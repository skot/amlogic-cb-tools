#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
RUST_TARGET="aarch64-unknown-linux-musl"
DESTINATION="${REPO_ROOT}/prebuilt/${RUST_TARGET}"

BINARIES=(
	apw12-psu-tool
	controlboard-misc
	fan-tool
	gpio-bias
	hashboard_s19jpro
	i2c-probe
	oled-ssd1306
)

die() {
	echo "ERROR: $*" >&2
	exit 1
}

for command in cargo rustup shasum; do
	command -v "${command}" >/dev/null 2>&1 ||
		die "Missing required command: ${command}"
done

if ! rustup target list --installed | grep -qx "${RUST_TARGET}"; then
	rustup target add "${RUST_TARGET}"
fi

BUILD_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/amlogic-cb-tools-prebuilt.XXXXXX")"
trap 'rm -rf "${BUILD_ROOT}"' EXIT

echo "== clean ${RUST_TARGET} build =="
(
	cd "${REPO_ROOT}"
	CARGO_TARGET_DIR="${BUILD_ROOT}/target" \
		cargo build --locked --release --target "${RUST_TARGET}"
)

mkdir -p "${DESTINATION}"
for binary in "${BINARIES[@]}"; do
	source_path="${BUILD_ROOT}/target/${RUST_TARGET}/release/${binary}"
	[[ -f "${source_path}" ]] || die "Missing build artifact: ${source_path}"
	install -m 0755 "${source_path}" "${DESTINATION}/${binary}"
done

echo "== checksums =="
(
	cd "${DESTINATION}"
	shasum -a 256 "${BINARIES[@]}" >SHA256SUMS
	shasum -a 256 -c SHA256SUMS
)

echo "Prebuilt binaries refreshed in ${DESTINATION}"
