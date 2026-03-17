#!/usr/bin/env bash

set -euo pipefail

usage() {
	cat <<'EOF'
Usage: usb/install-wifi-userspace.sh <board-ip>

Uploads the built Wi-Fi userspace tools to the board:

	/usr/local/bin/wpa_supplicant
	/usr/local/bin/wpa_cli
	/usr/local/bin/wpa_passphrase
	/lib/libnl-3.so.200
	/lib/libnl-genl-3.so.200

Environment:
	BOARD_PASSWORD  SSH password for the board (default: root)
EOF
}

if [[ ${1:-} == "-h" || ${1:-} == "--help" ]]; then
	usage
	exit 0
fi

if [[ $# -ne 1 ]]; then
	usage >&2
	exit 2
fi

if ! command -v sshpass >/dev/null 2>&1; then
	echo "sshpass is required" >&2
	exit 1
fi

board_ip=$1
board_password=${BOARD_PASSWORD:-root}
ssh_opts=(-o StrictHostKeyChecking=no)
repo_root=$(cd "$(dirname "$0")/.." && pwd)
artifact_dir="$repo_root/usb/bin"

for artifact in wpa_supplicant-armhf wpa_cli-armhf wpa_passphrase-armhf libnl-3.so.200 libnl-genl-3.so.200; do
	if [[ ! -f "$artifact_dir/$artifact" ]]; then
		echo "Missing artifact: $artifact_dir/$artifact" >&2
		exit 1
	fi
done

remote_ssh() {
	sshpass -p "$board_password" ssh "${ssh_opts[@]}" "root@$board_ip" "$@"
}

remote_run() {
	remote_ssh "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin; $*"
}

echo "== prepare remote install dir =="
remote_run "mkdir -p /usr/local/bin /lib"

for artifact in wpa_supplicant-armhf wpa_cli-armhf wpa_passphrase-armhf; do
	remote_name=${artifact%-armhf}
	echo "== upload $remote_name =="
	sshpass -p "$board_password" ssh "${ssh_opts[@]}" "root@$board_ip" "cat > /tmp/$remote_name" < "$artifact_dir/$artifact"
	remote_run "cp /tmp/$remote_name /usr/local/bin/$remote_name && chmod 0755 /usr/local/bin/$remote_name"
done

for artifact in libnl-3.so.200 libnl-genl-3.so.200; do
	echo "== upload $artifact =="
	sshpass -p "$board_password" ssh "${ssh_opts[@]}" "root@$board_ip" "cat > /tmp/$artifact" < "$artifact_dir/$artifact"
	remote_run "cp /tmp/$artifact /lib/$artifact && chmod 0644 /lib/$artifact"
done

echo "== verify installed tools =="
remote_run "LD_LIBRARY_PATH=/lib /usr/local/bin/wpa_supplicant -v; echo ---; LD_LIBRARY_PATH=/lib /usr/local/bin/wpa_cli -v; echo ---; ls -l /usr/local/bin/wpa_* /lib/libnl*.so.200"