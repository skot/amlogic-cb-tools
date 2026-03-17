#!/usr/bin/env bash

set -euo pipefail

usage() {
	cat <<'EOF'
Usage: usb/build-usb-storage-kernel.sh

Builds an arm64 Amlogic 4.9 kernel in Docker using usb/Antminer-4.9.241.config
as the baseline, then forces these options built-in:
	CONFIG_SCSI=y
	CONFIG_BLK_DEV_SD=y
	CONFIG_USB_STORAGE=y
	CONFIG_USB_UAS=y

Artifacts are written under usb/bin/:
	usb/bin/Image-usb-storage
	usb/bin/Image-usb-storage.sha256
	usb/bin/.config.final
	usb/bin/olddefconfig.log
	usb/bin/build.log
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
}
seen = set()
out = []
for line in lines:
		replaced = False
		for key, value in wanted.items():
				if line.startswith(f"{key}=") or line == f"# {key} is not set":
						out.append(f"{key}={value}")
						seen.add(key)
						replaced = True
						break
		if not replaced:
				out.append(line)
for key, value in wanted.items():
		if key not in seen:
				out.append(f"{key}={value}")
cfg.write_text("\n".join(out) + "\n")
PY

		mkdir -p /work/usb/bin

		make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- olddefconfig \
			> /work/usb/bin/olddefconfig.log 2>&1

		make -j"$(nproc)" ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- \
			KCFLAGS=-Wno-error Image \
			> /work/usb/bin/build.log 2>&1

		cp arch/arm64/boot/Image /work/usb/bin/Image-usb-storage
		cp .config /work/usb/bin/.config.final
		cd /work
		sha256sum usb/bin/Image-usb-storage > usb/bin/Image-usb-storage.sha256
	'

echo "Build complete."
echo "Image: $artifact_dir/Image-usb-storage"
echo "Config: $artifact_dir/.config.final"
echo "Logs: $artifact_dir/olddefconfig.log and $artifact_dir/build.log"