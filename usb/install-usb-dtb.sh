#!/usr/bin/env bash

set -euo pipefail

usage() {
	cat <<'EOF'
Usage: usb/install-usb-dtb.sh <board-ip> [dtb-path]

Default DTB:
	usb/bin/axg_s400_antminer.usb-host-nand-clocks.dtb
	This is the validated companion DTB for the rebuilt USB-storage kernel.

Environment:
	BOARD_PASSWORD   SSH password for the board (default: root)
	REMOTE_DTB_PATH  Remote DTB path to replace (default: /axg_s400_antminer.dtb)
	REMOTE_TMP_PATH  Temporary upload path (default: /tmp/axg_s400_antminer.usb-host-nand-clocks.dtb)
	REBOOT_AFTER_INSTALL  Set to 1 to reboot automatically after a successful install

Example:
	BOARD_PASSWORD=root usb/install-usb-dtb.sh <board-ip>
EOF
}

if [[ ${1:-} == "-h" || ${1:-} == "--help" ]]; then
	usage
	exit 0
fi

if [[ $# -lt 1 || $# -gt 2 ]]; then
	usage >&2
	exit 2
fi

if ! command -v sshpass >/dev/null 2>&1; then
	echo "sshpass is required" >&2
	exit 1
fi

if ! command -v shasum >/dev/null 2>&1; then
	echo "shasum is required" >&2
	exit 1
fi

board_ip=$1
board_password=${BOARD_PASSWORD:-root}
remote_dtb_path=${REMOTE_DTB_PATH:-/axg_s400_antminer.dtb}
remote_tmp_path=${REMOTE_TMP_PATH:-/tmp/axg_s400_antminer.usb-host-nand-clocks.dtb}
backup_suffix=.pre-usb-host
remote_backup_path="${remote_dtb_path}${backup_suffix}"
ssh_opts=(-o StrictHostKeyChecking=no)
repo_root=$(cd "$(dirname "$0")/.." && pwd)
artifact_dir="$repo_root/usb/bin"
dtb_path=${2:-$artifact_dir/axg_s400_antminer.usb-host-nand-clocks.dtb}

if [[ ! -f "$dtb_path" ]]; then
	echo "DTB file not found: $dtb_path" >&2
	exit 1
fi

local_sha=$(shasum -a 256 "$dtb_path" | awk '{print $1}')

remote_ssh() {
	sshpass -p "$board_password" ssh "${ssh_opts[@]}" "root@$board_ip" "$@"
}

rollback_needed=0

rollback() {
	if [[ $rollback_needed -eq 1 ]]; then
		echo "Install failed after backup; restoring ${remote_backup_path}" >&2
		remote_ssh "set -e; cp '${remote_backup_path}' '${remote_dtb_path}'; sync" || true
	fi
}

trap rollback EXIT

echo "== local artifact =="
echo "DTB: $dtb_path"
echo "SHA256: $local_sha"

echo "== remote preflight =="
remote_ssh "set -e; ls -l '${remote_dtb_path}'; sha256sum '${remote_dtb_path}'"

echo "== backup current DTB =="
remote_ssh "set -e; cp -a '${remote_dtb_path}' '${remote_backup_path}'; sha256sum '${remote_backup_path}'"
rollback_needed=1

echo "== upload patched DTB =="
sshpass -p "$board_password" ssh "${ssh_opts[@]}" "root@$board_ip" "cat > '${remote_tmp_path}'" < "$dtb_path"

echo "== verify uploaded DTB =="
remote_tmp_sha=$(remote_ssh "sha256sum '${remote_tmp_path}' | awk '{print \$1}'")
echo "Remote tmp SHA256: $remote_tmp_sha"
if [[ "$remote_tmp_sha" != "$local_sha" ]]; then
	echo "Uploaded DTB checksum mismatch" >&2
	exit 1
fi

echo "== install patched DTB =="
remote_ssh "set -e; cp '${remote_tmp_path}' '${remote_dtb_path}'; sync"

echo "== verify installed DTB =="
remote_installed_sha=$(remote_ssh "sha256sum '${remote_dtb_path}' | awk '{print \$1}'")
echo "Remote installed SHA256: $remote_installed_sha"
if [[ "$remote_installed_sha" != "$local_sha" ]]; then
	echo "Installed DTB checksum mismatch" >&2
	exit 1
fi

rollback_needed=0

echo "Install complete. Backup saved as ${remote_backup_path}."
echo "Validated rebuilt-kernel pairing: $artifact_dir/axg_s400_antminer.usb-host-nand-clocks.dtb"

if [[ ${REBOOT_AFTER_INSTALL:-0} == "1" ]]; then
	echo "== reboot =="
	remote_ssh "/sbin/reboot || /bin/busybox reboot || busybox reboot || true" || true
	echo "Reboot requested."
else
	echo "Reboot not requested. Set REBOOT_AFTER_INSTALL=1 to reboot automatically."
fi