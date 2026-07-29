#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BOARD_HOST=""
BOARD_PORT="${BOARD_PORT:-22}"
BOARD_USER="${BOARD_USER:-miner}"
BOARD_PASSWORD="${BOARD_PASSWORD:-miner}"
SKIP_BOOTSTRAP=0
SKIP_BUILD=0
REMOTE_STAGE="/tmp/amlogic-cb-tools-installer"

BINARIES=(
	apw12-psu-tool
	controlboard-misc
	fan-tool
	gpio-bias
	hashboard_s19jpro
	i2c-probe
	oled-ssd1306
)

usage() {
	cat <<'EOF'
Usage:
  ./amlogic-cb-tools-installer.sh <board-ip> [options]

Bootstraps passwordless sudo on a stock Amlogic control board, builds the
tools for the board's architecture, and installs them in /usr/bin.

Options:
  --port PORT          SSH port (default: 22)
  --user USERNAME      Stock firmware SSH user (default: miner)
  --password PASSWORD  Stock firmware SSH password (default: miner)
  --skip-bootstrap     Require passwordless sudo to already be configured
  --skip-build         Install existing target artifacts without rebuilding
  --help               Show this message

Environment:
  BOARD_PORT           Default SSH port
  BOARD_USER           Default SSH username
  BOARD_PASSWORD       Default SSH password

Examples:
  ./amlogic-cb-tools-installer.sh 192.168.1.236
  BOARD_PASSWORD=miner ./amlogic-cb-tools-installer.sh 192.168.1.236
  ./amlogic-cb-tools-installer.sh 192.168.1.236 --skip-bootstrap

The installed tools access GPIO, I2C, PWM, serial, or /dev/mem. Run hardware
commands through sudo, for example:

  sudo controlboard-misc status
EOF
}

die() {
	echo "ERROR: $*" >&2
	exit 1
}

need_cmd() {
	command -v "$1" >/dev/null 2>&1 || die "Missing required command: $1"
}

need_value() {
	local option="$1"
	local value="${2:-}"
	[[ -n "${value}" ]] || die "Missing value for ${option}"
}

if [[ $# -eq 0 ]]; then
	usage >&2
	exit 2
fi

if [[ "${1}" == "--help" || "${1}" == "-h" ]]; then
	usage
	exit 0
fi

BOARD_HOST="$1"
shift

while [[ $# -gt 0 ]]; do
	case "$1" in
		--port)
			need_value "$1" "${2:-}"
			BOARD_PORT="$2"
			shift 2
			;;
		--user)
			need_value "$1" "${2:-}"
			BOARD_USER="$2"
			shift 2
			;;
		--password)
			need_value "$1" "${2:-}"
			BOARD_PASSWORD="$2"
			shift 2
			;;
		--skip-bootstrap)
			SKIP_BOOTSTRAP=1
			shift
			;;
		--skip-build)
			SKIP_BUILD=1
			shift
			;;
		--help|-h)
			usage
			exit 0
			;;
		*)
			die "Unknown argument: $1"
			;;
	esac
done

[[ -n "${BOARD_HOST}" ]] || die "Board IP address or hostname is required"
[[ "${BOARD_PORT}" =~ ^[0-9]+$ ]] || die "Invalid SSH port: ${BOARD_PORT}"
[[ "${BOARD_USER}" =~ ^[a-z_][a-z0-9_-]*$ ]] ||
	die "Invalid SSH username: ${BOARD_USER}"

need_cmd ssh
need_cmd sshpass
need_cmd shasum
need_cmd tar

SSH_TARGET="${BOARD_USER}@${BOARD_HOST}"
SSH_BASE=(
	sshpass -e
	ssh
	-o PreferredAuthentications=password
	-o PubkeyAuthentication=no
	-o StrictHostKeyChecking=no
	-o UserKnownHostsFile=/dev/null
	-o LogLevel=ERROR
	-o ConnectTimeout=10
	-p "${BOARD_PORT}"
	"${SSH_TARGET}"
)

remote_run() {
	local command="$1"
	SSHPASS="${BOARD_PASSWORD}" "${SSH_BASE[@]}" \
		"sh -lc $(printf '%q' "${command}")" </dev/null
}

remote_try() {
	local command="$1"
	set +e
	SSHPASS="${BOARD_PASSWORD}" "${SSH_BASE[@]}" \
		"sh -lc $(printf '%q' "${command}")" </dev/null
	local result=$?
	set -e
	return "${result}"
}

cleanup_remote_stage() {
	remote_try "sudo -n rm -rf '${REMOTE_STAGE}'" >/dev/null 2>&1 || true
}

trap cleanup_remote_stage EXIT

echo "== connect =="
echo "Target: ${SSH_TARGET}:${BOARD_PORT}"
remote_run 'echo "identity=$(id)"; echo "kernel=$(uname -a)"'

if remote_try 'sudo -n true' >/dev/null 2>&1; then
	echo "Passwordless sudo is already configured."
elif [[ "${SKIP_BOOTSTRAP}" == "1" ]]; then
	die "Passwordless sudo is unavailable and --skip-bootstrap was requested"
else
	echo "== bootstrap passwordless sudo =="
	remote_run "command -v daemonc >/dev/null 2>&1 || {
		echo 'daemonc is unavailable; this does not look like supported stock firmware' >&2
		exit 1
	}"
	remote_run "daemonc \"\\\`echo '${BOARD_USER} ALL=NOPASSWD:ALL'>>/etc/sudoers && chmod +s /usr/bin/sudo\\\`\" || true"
	remote_run "sudo -n true" ||
		die "Bootstrap payload completed but passwordless sudo verification failed"
fi

echo "== remote preflight =="
REMOTE_ARCH="$(remote_run 'uname -m' | tr -d '\r\n')"
case "${REMOTE_ARCH}" in
	aarch64|arm64)
		RUST_TARGET="aarch64-unknown-linux-musl"
		BUILD_COMMAND=(cargo build --locked --release --target "${RUST_TARGET}")
		;;
	armv7l|armv7)
		RUST_TARGET="armv7-unknown-linux-gnueabihf"
		BUILD_COMMAND=(cargo zigbuild --locked --release --target "${RUST_TARGET}")
		;;
	*)
		die "Unsupported board architecture: ${REMOTE_ARCH}"
		;;
esac

echo "Architecture: ${REMOTE_ARCH}"
echo "Rust target: ${RUST_TARGET}"
remote_run '
	set -eu
	test "$(sudo -n id -u)" = "0"
	test -d /usr/bin
	test -w /tmp
	for command in sha256sum tar; do
		command -v "${command}" >/dev/null
	done
'

if [[ "${SKIP_BUILD}" != "1" ]]; then
	echo "== build =="
	need_cmd cargo
	need_cmd rustup
	if [[ "${RUST_TARGET}" == "armv7-unknown-linux-gnueabihf" ]]; then
		need_cmd zig
		need_cmd cargo-zigbuild
	fi
	if ! rustup target list --installed | grep -qx "${RUST_TARGET}"; then
		rustup target add "${RUST_TARGET}"
	fi
	(
		cd "${SCRIPT_DIR}"
		"${BUILD_COMMAND[@]}"
	)
else
	echo "== build skipped =="
fi

ARTIFACT_DIR="${SCRIPT_DIR}/target/${RUST_TARGET}/release"
for binary in "${BINARIES[@]}"; do
	[[ -f "${ARTIFACT_DIR}/${binary}" ]] ||
		die "Missing build artifact: ${ARTIFACT_DIR}/${binary}"
done

echo "== checksums =="
CHECKSUM_MANIFEST=""
for binary in "${BINARIES[@]}"; do
	checksum="$(shasum -a 256 "${ARTIFACT_DIR}/${binary}" | awk '{print $1}')"
	CHECKSUM_MANIFEST+="${checksum}  ${binary}"$'\n'
	printf '%s  %s\n' "${checksum}" "${binary}"
done

echo "== upload =="
remote_run "sudo -n rm -rf '${REMOTE_STAGE}' && mkdir -p '${REMOTE_STAGE}'"
tar -C "${ARTIFACT_DIR}" -cf - "${BINARIES[@]}" |
	SSHPASS="${BOARD_PASSWORD}" "${SSH_BASE[@]}" \
		"tar -C '${REMOTE_STAGE}' -xf -"
printf '%s' "${CHECKSUM_MANIFEST}" |
	SSHPASS="${BOARD_PASSWORD}" "${SSH_BASE[@]}" \
		"cat > '${REMOTE_STAGE}/SHA256SUMS'"

echo "== verify upload =="
remote_run "cd '${REMOTE_STAGE}' && sha256sum -c SHA256SUMS"

echo "== install =="
remote_run "
	set -eu
	for binary in ${BINARIES[*]}; do
		destination=\"/usr/bin/\${binary}\"
		replacement=\"/usr/bin/.\${binary}.amlogic-cb-tools-installer\"
		sudo -n rm -f \"\${replacement}\"
		sudo -n cp '${REMOTE_STAGE}'/\"\${binary}\" \"\${replacement}\"
		sudo -n chown root:root \"\${replacement}\"
		sudo -n chmod 0755 \"\${replacement}\"
		sudo -n mv -f \"\${replacement}\" \"\${destination}\"
	done
	sync
"

echo "== verify installation =="
remote_run "
	set -eu
	cd /usr/bin
	for binary in ${BINARIES[*]}; do
		test \"\$(command -v \"\${binary}\")\" = \"/usr/bin/\${binary}\"
		test -x \"/usr/bin/\${binary}\"
		sudo -n \"/usr/bin/\${binary}\" --help >/dev/null
	done
	printf '%s' '${CHECKSUM_MANIFEST}' >/tmp/amlogic-cb-tools-installed.SHA256SUMS
	sha256sum -c /tmp/amlogic-cb-tools-installed.SHA256SUMS
	rm -f /tmp/amlogic-cb-tools-installed.SHA256SUMS
"

cleanup_remote_stage
trap - EXIT

echo
echo "Installation complete on ${BOARD_HOST}."
echo "Installed ${#BINARIES[@]} tools in /usr/bin."
echo "Run hardware commands with sudo, for example:"
echo "  sudo controlboard-misc status"
