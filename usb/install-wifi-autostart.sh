#!/usr/bin/env bash

set -euo pipefail

usage() {
	cat <<'EOF'
Usage: usb/install-wifi-autostart.sh <board-ip>

Installs a SysV init script on the board that automatically:
	- loads the RTL8821CU module
	- starts wpa_supplicant on wlan0 using /config/wpa_supplicant-wlan0.conf
	- requests DHCP on wlan0
	- applies source-based routing for the Wi-Fi address
	- drops stale eth0 IPv4 when Ethernet has no carrier
	- patches LuxOS networking to skip the long eth0 DHCP wait when Ethernet
	  has no carrier and Wi-Fi autostart is configured

Environment:
	BOARD_PASSWORD      SSH password for the board (default: root)
	START_AFTER_INSTALL Set to 1 to start/restart the service immediately
	                    after install (default: 0)
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
start_after_install=${START_AFTER_INSTALL:-0}
ssh_opts=(-o StrictHostKeyChecking=no)
repo_root=$(cd "$(dirname "$0")/.." && pwd)
service_src="$repo_root/usb/wifi-autostart.init"
network_patch_src="$repo_root/usb/patch-networking-fast-fail.pl"

if [[ ! -f "$service_src" ]]; then
	echo "Missing service template: $service_src" >&2
	exit 1
fi

if [[ ! -f "$network_patch_src" ]]; then
	echo "Missing networking patch helper: $network_patch_src" >&2
	exit 1
fi

remote_ssh() {
	sshpass -p "$board_password" ssh "${ssh_opts[@]}" "root@$board_ip" "$@"
}

remote_run() {
	remote_ssh "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin; $*"
}

echo "== upload service =="
sshpass -p "$board_password" ssh "${ssh_opts[@]}" "root@$board_ip" "cat > /tmp/wifi-autostart" < "$service_src"

echo "== upload networking patch helper =="
sshpass -p "$board_password" ssh "${ssh_opts[@]}" "root@$board_ip" "cat > /tmp/patch-networking-fast-fail.pl" < "$network_patch_src"

echo "== install service =="
remote_run "set -e; cp /tmp/wifi-autostart /etc/init.d/wifi-autostart; chmod 0755 /etc/init.d/wifi-autostart; \
	for level in 2 3 4 5; do rm -f /etc/rc\${level}.d/S11wifi-autostart; ln -sf ../init.d/wifi-autostart /etc/rc\${level}.d/S02wifi-autostart; done; \
	for level in 0 1 6; do ln -sf ../init.d/wifi-autostart /etc/rc\${level}.d/K89wifi-autostart; done"

echo "== patch networking fast-fail =="
remote_run "chmod 0755 /tmp/patch-networking-fast-fail.pl; /usr/bin/perl /tmp/patch-networking-fast-fail.pl /etc/init.d/networking; chmod 0755 /etc/init.d/networking"

echo "== verify service =="
remote_run "ls -l /etc/init.d/wifi-autostart /etc/rc2.d/S02wifi-autostart /etc/rc5.d/S02wifi-autostart /etc/rc0.d/K89wifi-autostart; echo ---; grep -n 'WIFI_FASTBOOT_WAIT_GUARD\|WIFI_FASTBOOT_REVERT_GUARD\|skipping eth0 DHCP wait' /etc/init.d/networking || true"

if [[ "$start_after_install" == "1" ]]; then
	echo "== start service =="
	remote_run "/etc/init.d/wifi-autostart restart || /etc/init.d/wifi-autostart start"
	remote_run "/etc/init.d/wifi-autostart status"
else
	echo "Service installed and enabled for boot. Set START_AFTER_INSTALL=1 to start it immediately."
fi