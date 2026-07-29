# Checked-in prebuilt binaries

The installer uses these binaries by default so installing onto a stock board
does not require a Rust toolchain or a GitHub release download.

Currently bundled target:

- `aarch64-unknown-linux-musl` for stock Amlogic control-board firmware

Regenerate the binaries and their checksum manifest from a clean source build:

```sh
./scripts/refresh-prebuilt.sh
```

Do not edit the generated target directory by hand. Commit source changes,
refreshed binaries, and the updated `SHA256SUMS` together.
