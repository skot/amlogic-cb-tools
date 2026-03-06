# amlogic control-board tools

Standalone Rust utilities for talking directly to hardware on an Amlogic-based S19j Pro control board.

## Purpose

This project is intentionally separate from `mujina-test`. It is meant to host small standalone binaries that can be deployed onto a live control board so hardware behavior can be validated independently before integrating support into Mujina.

## Binaries

- `apw12-psu-tool` — APW12 PSU control and telemetry
- `fan-tool` — fan PWM and tachometer experiments for the Amlogic control board

## Current APW12 scope

- opens the dedicated PSU bus at `/dev/i2c-1`
- defaults to the live LuxOS-facing transport at address `0x10`
- defaults to outbound byte writes via register `0x11`
- sends and receives framed APW12 protocol packets one byte per I2C transaction
- supports basic protocol commands such as:
  - address scanning on the dedicated PSU bus
  - firmware/hardware version
  - watchdog enable/disable
  - DAC setpoint readback
  - measured voltage readback
  - state readback
  - calibration reads
- includes a `prepare-board` helper that drives PSU enable GPIO `437` high via sysfs

Shared code lives in the library crate under `src/lib.rs`, with each executable in `src/bin/`.

## Current fan-tool scope

- controls PWM channels on `/sys/class/pwm/pwmchip0/pwm0` and `pwm1`
- assumes a 100 kHz PWM period (`10000` ns)
- reads tachometer inputs from GPIO `447` through `450`
- measures RPM by counting falling edges with Linux sysfs GPIO polling
- supports these commands:
  - `fan-tool get-pwm`
  - `fan-tool set-percent <percent>`
  - `fan-tool set-pwm <all|0|1> <percent>`
  - `fan-tool read-rpm [all|0|1|2|3] [window_ms]`

Current assumptions were validated on the live LuxOS Amlogic control board with fans connected:

- PWM writes to `pwmchip0/pwm0` and `pwm1` successfully change fan speed
- tachometer GPIOs `447` through `450` report non-zero RPM when fans are connected
- fan speed changes take a short time to settle, so RPM should be sampled after a delay

Example live workflow:

- `/home/root/fan-tool set-percent 15`
- `sleep 3`
- `/home/root/fan-tool read-rpm all 1500`

## Build

Recommended target:

- `aarch64-unknown-linux-musl`

Example:

- `cargo build --release --target aarch64-unknown-linux-musl`

Build one binary explicitly:

- `cargo build --release --target aarch64-unknown-linux-musl --bin apw12-psu-tool`
- `cargo build --release --target aarch64-unknown-linux-musl --bin fan-tool`

## Deploy

Example copy target:

- `/home/root/apw12-psu-tool`
- `/home/root/fan-tool`

The corresponding compiled binaries will appear under:

- `target/<triple>/release/apw12-psu-tool`
- `target/<triple>/release/fan-tool`

## Disable LuxOS mining tools first

Before running the tools on a live LuxOS control board, disable `luxminer` and
`luxupdate` so they do not interfere with PSU control, fan control, or other
hardware access.

This project includes a helper script:

- [disable_luxminer.exp](disable_luxminer.exp)

Usage:

- `chmod +x disable_luxminer.exp`
- `./disable_luxminer.exp <board-ip>`

Example:

- `./disable_luxminer.exp 192.168.1.236`

What it does:

- kills any running `luxminer` / `luxupdate` processes
- replaces `/luxminer` and `/luxupdate` with inert symlinks to `/bin/false`
- preserves the original binaries as backup files
- replaces the LuxOS init script with a no-op stub
- disables the LuxOS runlevel startup links

The original LuxOS binaries are preserved and not deleted.

After disabling LuxOS mining tools, deploy and run the binaries from this
project.

## Safety notes

- Disable the watchdog before longer experiments.
- Avoid `WRITE_CAL` until the calibration map is fully understood.
- Voltage changes should be followed by a short settling delay before measurement.
- Fan speed changes should also be followed by a short settling delay before reading RPM.
- `fan-tool` currently assumes 2 tach pulses per revolution and two shared PWM channels for four fans.
