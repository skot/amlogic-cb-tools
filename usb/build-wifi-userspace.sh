#!/usr/bin/env bash

set -euo pipefail

usage() {
	cat <<'EOF'
Usage: usb/build-wifi-userspace.sh

Builds armhf Wi-Fi userspace tools for the LuxOS image on the board:

	usb/bin/wpa_supplicant-armhf
	usb/bin/wpa_cli-armhf
	usb/bin/wpa_passphrase-armhf
	usb/bin/libnl-3.so.200
	usb/bin/libnl-genl-3.so.200
	usb/bin/wifi-userspace.build.log

The build enables both `nl80211` and `wext` so it can talk to the cfg80211-based
RTL8821CU driver. The board already has OpenSSL 1.1, but it does not ship the
required `libnl` runtime, so the needed armhf `libnl` shared libraries are
exported alongside the binaries.
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
docker_image=debian:bullseye
wpa_version=${WPA_SUPPLICANT_VERSION:-2.10}

mkdir -p "$artifact_dir"

docker run --rm \
	-v "$repo_root:/work" \
	-e WPA_SUPPLICANT_VERSION="$wpa_version" \
	"$docker_image" \
	bash -lc '
		set -euo pipefail
		export DEBIAN_FRONTEND=noninteractive
		apt-get update
		apt-get install -y --no-install-recommends \
			build-essential ca-certificates dpkg-dev file gcc-arm-linux-gnueabihf \
			libc6-dev-armhf-cross pkg-config wget
		dpkg --add-architecture armhf
		apt-get update
		apt-get install -y --no-install-recommends \
			libnl-3-dev:armhf libnl-genl-3-dev:armhf libssl-dev:armhf zlib1g-dev:armhf

		rm -rf /tmp/wpa-build
		mkdir -p /tmp/wpa-build
		cd /tmp/wpa-build
		wget -O wpa_supplicant.tar.gz "https://w1.fi/releases/wpa_supplicant-${WPA_SUPPLICANT_VERSION}.tar.gz"
		tar xf wpa_supplicant.tar.gz
		cd "wpa_supplicant-${WPA_SUPPLICANT_VERSION}/wpa_supplicant"
		mkdir -p /work/usb/bin

		cat > .config <<"EOF"
CONFIG_CTRL_IFACE=y
CONFIG_CTRL_IFACE_UNIX=y
CONFIG_DRIVER_NL80211=y
CONFIG_DRIVER_WEXT=y
CONFIG_LIBNL32=y
CONFIG_TLS=openssl
CONFIG_BACKEND=file
EOF

		export PKG_CONFIG_LIBDIR=/usr/lib/arm-linux-gnueabihf/pkgconfig:/usr/share/pkgconfig
		export PKG_CONFIG_SYSROOT_DIR=/
		make clean >/dev/null 2>&1 || true
		make \
			CC=arm-linux-gnueabihf-gcc \
			EXTRA_CFLAGS="-I/usr/include/arm-linux-gnueabihf" \
			LDFLAGS="-L/usr/lib/arm-linux-gnueabihf" \
			wpa_supplicant wpa_cli wpa_passphrase \
			> /work/usb/bin/wifi-userspace.build.log 2>&1

		file wpa_supplicant wpa_cli wpa_passphrase
		cp wpa_supplicant /work/usb/bin/wpa_supplicant-armhf
		cp wpa_cli /work/usb/bin/wpa_cli-armhf
		cp wpa_passphrase /work/usb/bin/wpa_passphrase-armhf
		cp /lib/arm-linux-gnueabihf/libnl-3.so.200 /work/usb/bin/libnl-3.so.200
		cp /lib/arm-linux-gnueabihf/libnl-genl-3.so.200 /work/usb/bin/libnl-genl-3.so.200
		cd /work
		sha256sum usb/bin/wpa_supplicant-armhf > usb/bin/wpa_supplicant-armhf.sha256
		sha256sum usb/bin/wpa_cli-armhf > usb/bin/wpa_cli-armhf.sha256
		sha256sum usb/bin/wpa_passphrase-armhf > usb/bin/wpa_passphrase-armhf.sha256
		sha256sum usb/bin/libnl-3.so.200 > usb/bin/libnl-3.so.200.sha256
		sha256sum usb/bin/libnl-genl-3.so.200 > usb/bin/libnl-genl-3.so.200.sha256
	'

echo "Build complete."
echo "Artifacts:"
echo "  $artifact_dir/wpa_supplicant-armhf"
echo "  $artifact_dir/wpa_cli-armhf"
echo "  $artifact_dir/wpa_passphrase-armhf"
echo "  $artifact_dir/libnl-3.so.200"
echo "  $artifact_dir/libnl-genl-3.so.200"
echo "Log: $artifact_dir/wifi-userspace.build.log"