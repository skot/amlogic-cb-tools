#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
VERSION="${1:-}"
DIST_DIR="${2:-${REPO_ROOT}/dist}"
RUST_TARGET="aarch64-unknown-linux-musl"
PREBUILT_DIR="${REPO_ROOT}/prebuilt/${RUST_TARGET}"

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

[[ -n "${VERSION}" ]] || die "Usage: $0 VERSION [DIST_DIR]"
[[ "${VERSION}" =~ ^[A-Za-z0-9._-]+$ ]] || die "Invalid version: ${VERSION}"

for command in shasum tar; do
	command -v "${command}" >/dev/null 2>&1 ||
		die "Missing required command: ${command}"
done

[[ -f "${PREBUILT_DIR}/SHA256SUMS" ]] ||
	die "Missing ${PREBUILT_DIR}/SHA256SUMS"

echo "== verify checked-in prebuilts =="
(
	cd "${PREBUILT_DIR}"
	shasum -a 256 -c SHA256SUMS
)

mkdir -p "${DIST_DIR}"
ARCHIVE_NAME="amlogic-cb-tools-${VERSION}-${RUST_TARGET}.tar.gz"
ARCHIVE_PATH="${DIST_DIR}/${ARCHIVE_NAME}"

echo "== package ${ARCHIVE_NAME} =="
tar -C "${PREBUILT_DIR}" -czf "${ARCHIVE_PATH}" \
	SHA256SUMS "${BINARIES[@]}"

(
	cd "${DIST_DIR}"
	shasum -a 256 "${ARCHIVE_NAME}" >SHA256SUMS
	shasum -a 256 -c SHA256SUMS
)

echo "Release assets written to ${DIST_DIR}"
