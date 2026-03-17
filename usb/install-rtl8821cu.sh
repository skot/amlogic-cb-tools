#!/usr/bin/env bash

set -euo pipefail

usage() {
	cat <<'EOF'
Usage: usb/install-rtl8821cu.sh <board-ip> [module-path]

Installs the external RTL8821CU kernel module on the live board and attempts
to load it with modprobe.

Environment:
	BOARD_PASSWORD      SSH password for the board (default: root)
	LOAD_AFTER_INSTALL  Set to 0 to skip modprobe after upload (default: 1)

Example:
	BOARD_PASSWORD=root usb/install-rtl8821cu.sh <board-ip>
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
load_after_install=${LOAD_AFTER_INSTALL:-1}
ssh_opts=(-o StrictHostKeyChecking=no)
repo_root=$(cd "$(dirname "$0")/.." && pwd)
artifact_dir="$repo_root/usb/bin"
module_path=${2:-$artifact_dir/8821cu.ko}

if [[ ! -f "$module_path" ]]; then
	echo "Module not found: $module_path" >&2
	exit 1
fi

local_sha=$(shasum -a 256 "$module_path" | awk '{print $1}')

remote_ssh() {
	sshpass -p "$board_password" ssh "${ssh_opts[@]}" "root@$board_ip" "$@"
}

remote_run() {
	remote_ssh "PATH=/usr/sbin:/usr/bin:/sbin:/bin; $*"
}

remote_kernel_release=$(remote_ssh 'uname -r')
remote_module_dir="/lib/modules/${remote_kernel_release}/kernel/drivers/net/wireless/realtek/rtl8821cu"
remote_module_path="${remote_module_dir}/8821cu.ko"
remote_tmp_path="/tmp/8821cu.ko"

echo "== local artifact =="
echo "Module: $module_path"
echo "SHA256: $local_sha"

echo "== remote preflight =="
echo "Kernel release: $remote_kernel_release"
remote_run "mkdir -p '${remote_module_dir}'"

echo "== upload module =="
sshpass -p "$board_password" ssh "${ssh_opts[@]}" "root@$board_ip" "cat > '${remote_tmp_path}'" < "$module_path"

echo "== verify uploaded module =="
remote_tmp_sha=$(remote_ssh "sha256sum '${remote_tmp_path}' | awk '{print \$1}'")
echo "Remote tmp SHA256: $remote_tmp_sha"
if [[ "$remote_tmp_sha" != "$local_sha" ]]; then
	echo "Uploaded module checksum mismatch" >&2
	exit 1
fi

echo "== install module =="
remote_run "set -e; cp '${remote_tmp_path}' '${remote_module_path}'; chmod 0644 '${remote_module_path}'; /sbin/depmod -a '${remote_kernel_release}' || true"

if [[ "$load_after_install" == "1" ]]; then
	echo "== load module =="
	remote_run "/sbin/modprobe 8821cu || /sbin/insmod '${remote_module_path}'"

	echo "== post-load summary =="
	remote_run "set -e; uname -a; echo ---; lsmod | grep -E '^(8821cu|cfg80211)' || true; echo ---; ls /sys/class/net; echo ---; dmesg | tail -n 80 | grep -Ei '8821cu|cfg80211|wlan|usb 1-1|rtl|wireless_send_event' || true"
else
	echo "Module installed but not loaded. Set LOAD_AFTER_INSTALL=1 to load it automatically."
fi