#!/usr/bin/env bash

set -euo pipefail

usage() {
	cat <<'EOF'
Usage: usb/build-rtl8821cu-module.sh

Builds the external brektrou RTL8811CU/RTL8821CU driver against the same
LineageOS Amlogic 4.9 kernel tree used for the USB-enabled board image.

Environment:
	RTL8821CU_USE_CFG80211  Set to 1 to keep cfg80211 support enabled in the
	                        driver build. Default: 1, which matches the
	                        Wi-Fi-capable kernel built by
	                        usb/build-usb-wifi-kernel.sh.
	                        Set to 0 only for manual experiments against a
	                        kernel that already exports legacy WEXT symbols.
use_cfg80211=${RTL8821CU_USE_CFG80211:-1}

Artifacts are written under usb/bin/:
	usb/bin/8821cu.ko
	usb/bin/8821cu.ko.sha256
	usb/bin/rtl8821cu.build.log
	usb/bin/.config.rtl8821cu.final
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
use_cfg80211=${RTL8821CU_USE_CFG80211:-1}
config_path="$usb_dir/Antminer-4.9.241.config"
if [[ "$use_cfg80211" == "1" && -f "$artifact_dir/.config.wifi.final" ]]; then
	config_path="$artifact_dir/.config.wifi.final"
elif [[ "$use_cfg80211" == "1" && -f "$usb_dir/.config.wifi.final" ]]; then
	config_path="$usb_dir/.config.wifi.final"
fi
docker_volume=amlogic-kernel-4_9-src
docker_image=ubuntu:22.04

if [[ ! -f "$config_path" ]]; then
	echo "Missing config: $config_path" >&2
	exit 1
fi

mkdir -p "$artifact_dir"

container_config_path="/work/${config_path#"$repo_root/"}"

docker run --rm \
	-v "$repo_root:/work" \
	-v "$docker_volume:/src" \
	-e KERNEL_CONFIG_PATH="$container_config_path" \
	-e RTL8821CU_USE_CFG80211="$use_cfg80211" \
	"$docker_image" \
	bash -lc '
		set -euo pipefail
		export DEBIAN_FRONTEND=noninteractive
		apt-get update
		apt-get install -y --no-install-recommends \
			bc bison build-essential ca-certificates file flex git kmod \
			libelf-dev libncurses-dev libssl-dev make perl python3 rsync \
			gcc-aarch64-linux-gnu libc6-dev-arm64-cross xz-utils

		if [[ ! -d /src/linux/.git ]]; then
			git clone --depth 1 https://github.com/LineageOS/android_kernel_amlogic_linux-4.9.git /src/linux
		fi

		rm -rf /tmp/rtl8821CU
		git clone --depth 1 https://github.com/brektrou/rtl8821CU.git /tmp/rtl8821CU

		cd /src/linux
		cp "$KERNEL_CONFIG_PATH" .config

		python3 - <<"PY"
import os
from pathlib import Path

cfg = Path(".config")
lines = cfg.read_text().splitlines()
wanted = {
	"CONFIG_SCSI": "y",
	"CONFIG_BLK_DEV_SD": "y",
	"CONFIG_USB_STORAGE": "y",
	"CONFIG_USB_UAS": "y",
}
if os.environ.get("RTL8821CU_USE_CFG80211") == "1":
	wanted.update({
		"CONFIG_WLAN": "y",
		"CONFIG_CFG80211": "y",
		"CONFIG_CFG80211_WEXT": "y",
		"CONFIG_CFG80211_INTERNAL_REGDB": "y",
		"CONFIG_CFG80211_CRDA_SUPPORT": "n",
		"CONFIG_RFKILL": "y",
		"CONFIG_WIRELESS_EXT": "y",
		"CONFIG_WEXT_PRIV": "y",
		"CONFIG_WEXT_SPY": "y",
	})
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

		make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- olddefconfig >/dev/null 2>&1
		make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- modules_prepare >/dev/null 2>&1
		mkdir -p /work/usb/bin
		cp .config /work/usb/bin/.config.rtl8821cu.final

		cd /tmp/rtl8821CU
		python3 - <<"PY"
from pathlib import Path

makefile = Path("Makefile")
text = makefile.read_text()
text = text.replace("CONFIG_PLATFORM_I386_PC = y", "CONFIG_PLATFORM_I386_PC = n")
text = text.replace("CONFIG_PLATFORM_AML_S905 = n", "CONFIG_PLATFORM_AML_S905 = y")
makefile.write_text(text)
PY

		if [[ "$RTL8821CU_USE_CFG80211" != "1" ]]; then
			python3 - <<"PY"
from pathlib import Path

makefile = Path("Makefile")
text = makefile.read_text()
text = text.replace(
	"EXTRA_CFLAGS += -DCONFIG_IOCTL_CFG80211 -DRTW_USE_CFG80211_STA_EVENT\n",
	"",
)
makefile.write_text(text)
PY
		fi

		make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- KSRC=/src/linux -j"$(nproc)" \
			> /work/usb/bin/rtl8821cu.build.log 2>&1

		file 8821cu.ko
		cp 8821cu.ko /work/usb/bin/8821cu.ko
		cd /work
		sha256sum usb/bin/8821cu.ko > usb/bin/8821cu.ko.sha256
	'

echo "Build complete."
echo "Module: $artifact_dir/8821cu.ko"
echo "Config: $artifact_dir/.config.rtl8821cu.final"
echo "Log: $artifact_dir/rtl8821cu.build.log"