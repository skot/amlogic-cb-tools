#!/usr/bin/env bash

set -euo pipefail

usage() {
	cat <<'EOF'
Usage: usb/build-usb-wifi-kernel.sh

Builds an arm64 Amlogic 4.9 kernel in Docker using usb/Antminer-4.9.241.config
as the baseline, then forces these options for USB storage and Wi-Fi support:
	CONFIG_SCSI=y
	CONFIG_BLK_DEV_SD=y
	CONFIG_USB_STORAGE=y
	CONFIG_USB_UAS=y
	CONFIG_WLAN=y
	CONFIG_CFG80211=y
	CONFIG_CFG80211_WEXT=y
	CONFIG_CFG80211_INTERNAL_REGDB=y
	CONFIG_CFG80211_CRDA_SUPPORT=n
	CONFIG_RFKILL=y
	CONFIG_WIRELESS_EXT=y
	CONFIG_WEXT_PRIV=y
	CONFIG_WEXT_SPY=y

Artifacts are written under usb/bin/:
	usb/bin/Image-usb-storage-wifi
	usb/bin/Image-usb-storage-wifi.sha256
	usb/bin/.config.wifi.final
	usb/bin/olddefconfig-wifi.log
	usb/bin/build-wifi.log
EOF
}

if [[ ${1:-} == "-h" || ${1:-} == "--help" ]]; then
	usage
	exit 0
fi

if ! command -v docker >/dev/null 2>&1; then
	echo "docker is required" >&2
	exit 1
fi

repo_root=$(cd "$(dirname "$0")/.." && pwd)
usb_dir="$repo_root/usb"
artifact_dir="$usb_dir/bin"
config_path="$usb_dir/Antminer-4.9.241.config"
docker_volume=amlogic-kernel-4_9-src
docker_image=ubuntu:22.04

if [[ ! -f "$config_path" ]]; then
	echo "Missing config: $config_path" >&2
	exit 1
fi

mkdir -p "$artifact_dir"

docker run --rm \
	-v "$repo_root:/work" \
	-v "$docker_volume:/src" \
	"$docker_image" \
	bash -lc '
		set -euo pipefail
		export DEBIAN_FRONTEND=noninteractive
		apt-get update
		apt-get install -y --no-install-recommends \
			bc bison build-essential ca-certificates flex git kmod \
			libelf-dev libncurses-dev libssl-dev make perl python3 rsync \
			gcc-aarch64-linux-gnu libc6-dev-arm64-cross xz-utils

		if [[ ! -d /src/linux/.git ]]; then
			git clone --depth 1 https://github.com/LineageOS/android_kernel_amlogic_linux-4.9.git /src/linux
		fi

		cd /src/linux
		cp /work/usb/Antminer-4.9.241.config .config

		python3 - <<"PY"
from pathlib import Path

cfg = Path(".config")
lines = cfg.read_text().splitlines()
wanted = {
	"CONFIG_SCSI": "y",
	"CONFIG_BLK_DEV_SD": "y",
	"CONFIG_USB_STORAGE": "y",
	"CONFIG_USB_UAS": "y",
	"CONFIG_WLAN": "y",
	"CONFIG_CFG80211": "y",
	"CONFIG_CFG80211_WEXT": "y",
	"CONFIG_CFG80211_INTERNAL_REGDB": "y",
	"CONFIG_CFG80211_CRDA_SUPPORT": "n",
	"CONFIG_RFKILL": "y",
	"CONFIG_WIRELESS_EXT": "y",
	"CONFIG_WEXT_PRIV": "y",
	"CONFIG_WEXT_SPY": "y",
}
seen = set()
out = []
for line in lines:
	replaced = False
	for key, value in wanted.items():
		if line.startswith(f"{key}=") or line == f"# {key} is not set":
			if value == "n":
				out.append(f"# {key} is not set")
			else:
				out.append(f"{key}={value}")
			seen.add(key)
			replaced = True
			break
	if not replaced:
		out.append(line)
for key, value in wanted.items():
	if key not in seen:
		if value == "n":
			out.append(f"# {key} is not set")
		else:
			out.append(f"{key}={value}")
cfg.write_text("\n".join(out) + "\n")
PY

		mkdir -p /work/usb/bin

		make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- olddefconfig \
			> /work/usb/bin/olddefconfig-wifi.log 2>&1

		make -j"$(nproc)" ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- \
			KCFLAGS=-Wno-error Image \
			> /work/usb/bin/build-wifi.log 2>&1

		cp arch/arm64/boot/Image /work/usb/bin/Image-usb-storage-wifi
		cp .config /work/usb/bin/.config.wifi.final
		cd /work
		sha256sum usb/bin/Image-usb-storage-wifi > usb/bin/Image-usb-storage-wifi.sha256
	'

echo "Build complete."
echo "Image: $artifact_dir/Image-usb-storage-wifi"
echo "Config: $artifact_dir/.config.wifi.final"
echo "Logs: $artifact_dir/olddefconfig-wifi.log and $artifact_dir/build-wifi.log"