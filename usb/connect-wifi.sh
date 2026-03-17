#!/usr/bin/env bash

set -euo pipefail

usage() {
	cat <<'EOF'
Usage: usb/connect-wifi.sh <board-ip>

Connects the live board to a WPA/WPA2-PSK network using wlan0 and BusyBox
udhcpc. The board must already have the Wi-Fi kernel/module path enabled and
the userspace tools installed with usb/install-wifi-userspace.sh.

Environment:
	BOARD_PASSWORD  SSH password for the board (default: root)
	WIFI_IFACE      Interface name (default: wlan0)
	WIFI_CONFIG_PATH
	                Remote wpa_supplicant config path.
	                Default: /config/wpa_supplicant-<iface>.conf
	WIFI_ROUTE_TABLE
	                Policy routing table number used for the Wi-Fi source
	                address. Default: 101
	WIFI_SSID       SSID to join. Optional if WIFI_CONFIG_PATH already exists.
	WIFI_PSK        WPA/WPA2 passphrase to use. Optional if WIFI_CONFIG_PATH
	                already exists.
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

if [[ -n ${WIFI_SSID:-} && -z ${WIFI_PSK:-} ]]; then
	echo "WIFI_PSK is required when WIFI_SSID is set" >&2
	exit 1
fi

if [[ -z ${WIFI_SSID:-} && -n ${WIFI_PSK:-} ]]; then
	echo "WIFI_SSID is required when WIFI_PSK is set" >&2
	exit 1
fi

board_ip=$1
board_password=${BOARD_PASSWORD:-root}
wifi_iface=${WIFI_IFACE:-wlan0}
wifi_config_path=${WIFI_CONFIG_PATH:-/config/wpa_supplicant-$wifi_iface.conf}
wifi_route_table=${WIFI_ROUTE_TABLE:-101}
ssh_opts=(-o StrictHostKeyChecking=no)

escape_double_quoted() {
	printf '%s' "$1" | sed 's/[\\"]/\\&/g'
}

config_text=
if [[ -n ${WIFI_SSID:-} ]]; then
	ssid_escaped=$(escape_double_quoted "$WIFI_SSID")
	psk_escaped=$(escape_double_quoted "$WIFI_PSK")
	config_text=$(cat <<EOF
ctrl_interface=/var/run/wpa_supplicant
update_config=1
network={
	ssid="$ssid_escaped"
	psk="$psk_escaped"
	key_mgmt=WPA-PSK
}
EOF
)
fi

remote_ssh() {
	sshpass -p "$board_password" ssh "${ssh_opts[@]}" "root@$board_ip" "$@"
}

if [[ -n ${config_text:-} ]]; then
	echo "== upload wpa_supplicant config =="
	sshpass -p "$board_password" ssh "${ssh_opts[@]}" "root@$board_ip" "cat > '$wifi_config_path' && chmod 600 '$wifi_config_path'" <<< "$config_text"
else
	echo "== use existing wpa_supplicant config =="
	remote_ssh "set -e; test -s '$wifi_config_path'; chmod 600 '$wifi_config_path'"
fi

echo "== start Wi-Fi client =="
remote_ssh "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin; \
	LD_LIBRARY_PATH=/lib; \
	set -e; \
	wpa_pid=/var/run/wpa_supplicant-$wifi_iface.pid; \
	dhcp_pid=/var/run/udhcpc.$wifi_iface.pid; \
	mkdir -p /var/run/wpa_supplicant; \
	if [ -f \"\$wpa_pid\" ]; then kill \"\$(cat \"\$wpa_pid\")\" 2>/dev/null || true; rm -f \"\$wpa_pid\"; fi; \
	if [ -f \"\$dhcp_pid\" ]; then kill \"\$(cat \"\$dhcp_pid\")\" 2>/dev/null || true; rm -f \"\$dhcp_pid\"; fi; \
	rm -f /var/run/wpa_supplicant/'$wifi_iface'; \
	ifconfig '$wifi_iface' down || true; \
	ifconfig '$wifi_iface' up; \
	/usr/local/bin/wpa_supplicant -B -D nl80211,wext -i '$wifi_iface' -c '$wifi_config_path' -P \"\$wpa_pid\"; \
	sleep 5; \
	/usr/local/bin/wpa_cli -i '$wifi_iface' status || true; \
	udhcpc -n -q -t 10 -T 3 -p \"\$dhcp_pid\" -i '$wifi_iface'; \
	wifi_ip=\$(ip -4 addr show dev '$wifi_iface' | awk '/inet / {print \$2; exit}'); \
	wifi_src=\${wifi_ip%/*}; \
	wifi_subnet=\$(ip -4 route show dev '$wifi_iface' scope link | awk 'NR==1 {print \$1; exit}'); \
	wifi_gw=\$(ip route show default | awk 'NR==1 {print \$3; exit}'); \
	if [ -n \"\$wifi_src\" ] && [ -n \"\$wifi_subnet\" ]; then \
		ip rule del from \"\$wifi_src/32\" table '$wifi_route_table' 2>/dev/null || true; \
		ip route flush table '$wifi_route_table' 2>/dev/null || true; \
		ip route add \"\$wifi_subnet\" dev '$wifi_iface' src \"\$wifi_src\" table '$wifi_route_table'; \
		if [ -n \"\$wifi_gw\" ]; then ip route add default via \"\$wifi_gw\" dev '$wifi_iface' table '$wifi_route_table' 2>/dev/null || true; fi; \
		ip rule add from \"\$wifi_src/32\" table '$wifi_route_table' priority 10000; \
	fi; \
	echo ---; \
	ifconfig '$wifi_iface' || true; \
	echo ---; \
	ip rule show || true; \
	echo ---; \
	ip route show table '$wifi_route_table' || true; \
	echo ---; \
	dmesg | tail -n 60 | grep -Ei 'wlan0|rtl8821cu|wpa|802.11' || true"