#!/usr/bin/env bash

set -euo pipefail

usage() {
	cat <<'EOF'
Usage: usb/install-kernel-image.sh <board-ip> [image-path]

Validated pairing:
	Install this kernel together with
	usb/axg_s400_antminer.usb-host-nand-clocks.dtb
	using usb/install-usb-dtb.sh.

Environment:
	BOARD_PASSWORD        SSH password for the board (default: root)
	REMOTE_IMAGE_PATH     Remote kernel image path to replace (default: /Image)
	REMOTE_TMP_IMAGE_PATH Temporary upload path (default: /tmp/Image.usb-storage)
	REBOOT_AFTER_INSTALL  Set to 1 to reboot automatically after install

Example:
	BOARD_PASSWORD=root REBOOT_AFTER_INSTALL=1 \
		usb/install-kernel-image.sh <board-ip>
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
image_path=${2:-usb/Image-usb-storage}
board_password=${BOARD_PASSWORD:-root}
remote_image_path=${REMOTE_IMAGE_PATH:-/Image}
remote_tmp_image_path=${REMOTE_TMP_IMAGE_PATH:-/tmp/Image.usb-storage}
backup_suffix=.pre-usb-storage
remote_backup_path="${remote_image_path}${backup_suffix}"
ssh_opts=(-o StrictHostKeyChecking=no)

if [[ ! -f "$image_path" ]]; then
	echo "Kernel image not found: $image_path" >&2
	exit 1
fi

local_sha=$(shasum -a 256 "$image_path" | awk '{print $1}')

remote_ssh() {
	sshpass -p "$board_password" ssh "${ssh_opts[@]}" "root@$board_ip" "$@"
}

rollback_needed=0

rollback() {
	if [[ $rollback_needed -eq 1 ]]; then
		echo "Install failed after backup; restoring ${remote_backup_path}" >&2
		remote_ssh "set -e; cp '${remote_backup_path}' '${remote_image_path}'; sync" || true
	fi
}

trap rollback EXIT

echo "== local artifact =="
echo "Image: $image_path"
echo "SHA256: $local_sha"

echo "== remote preflight =="
remote_ssh "set -e; ls -l '${remote_image_path}'; sha256sum '${remote_image_path}'"

echo "== backup current Image =="
remote_ssh "set -e; cp -a '${remote_image_path}' '${remote_backup_path}'; sha256sum '${remote_backup_path}'"
rollback_needed=1

echo "== upload replacement Image =="
sshpass -p "$board_password" ssh "${ssh_opts[@]}" "root@$board_ip" "cat > '${remote_tmp_image_path}'" < "$image_path"

echo "== verify uploaded Image =="
remote_tmp_sha=$(remote_ssh "sha256sum '${remote_tmp_image_path}' | awk '{print \$1}'")
echo "Remote tmp SHA256: $remote_tmp_sha"
if [[ "$remote_tmp_sha" != "$local_sha" ]]; then
	echo "Uploaded Image checksum mismatch" >&2
	exit 1
fi

echo "== install replacement Image =="
remote_ssh "set -e; cp '${remote_tmp_image_path}' '${remote_image_path}'; sync"

echo "== verify installed Image =="
remote_installed_sha=$(remote_ssh "sha256sum '${remote_image_path}' | awk '{print \$1}'")
echo "Remote installed SHA256: $remote_installed_sha"
if [[ "$remote_installed_sha" != "$local_sha" ]]; then
	echo "Installed Image checksum mismatch" >&2
	exit 1
fi

rollback_needed=0

echo "Install complete. Backup saved as ${remote_backup_path}."
echo "Validated DTB pairing: usb/axg_s400_antminer.usb-host-nand-clocks.dtb"

if [[ ${REBOOT_AFTER_INSTALL:-0} == "1" ]]; then
	echo "== reboot =="
	remote_ssh "/sbin/reboot || /bin/busybox reboot || busybox reboot || true" || true
	echo "Reboot requested."
else
	echo "Reboot not requested. Set REBOOT_AFTER_INSTALL=1 to reboot automatically."
fi