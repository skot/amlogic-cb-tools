# amlogic control-board tools

Standalone Rust utilities for talking directly to hardware on an Amlogic A113D based Antminer control board that has been previously flashed with LuxOS. The LuxOS miner application needs to be disabled before running these tools.

## Purpose

This project is intentionally separate from [Mujina](https://github.com/256foundation/mujina/). It is meant to host small standalone binaries that can be deployed onto a live control board so hardware behavior can be validated independently before integrating support into Mujina.

## Binaries

- `apw12-psu-tool` — APW12 PSU control and telemetry
- `controlboard-misc` — misc GPIO helpers for control-board LEDs
- `fan-tool` — fan PWM and tachometer experiments for the Amlogic control board
- `hashboard_s19jpro` — direct serial and GPIO sanity checks for the three connected S19j Pro hashboards

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

## Current apw12-psu-tool scope

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

## Current controlboard-misc scope

- controls the two misc control-board LEDs via sysfs GPIO
- uses green LED GPIO `453`
- uses red LED GPIO `438`
- supports these commands:
  - `controlboard-misc status`
  - `controlboard-misc set <green|red|all> <on|off|high|low|1|0>`
  - `controlboard-misc toggle [green|red|all]`

Example:

- `/home/root/controlboard-misc status`
- `/home/root/controlboard-misc set green on`
- `/home/root/controlboard-misc toggle red`
- `/home/root/controlboard-misc toggle all`

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

## Current hashboard_s19jpro scope

- targets the three fixed Amlogic UARTs:
  - `/dev/ttyS1`
  - `/dev/ttyS2`
  - `/dev/ttyS3`
- reads hashboard TMP75 sensors directly over native Linux I2C on `/dev/i2c-0`
- uses reset GPIOs `454`, `455`, `456`
- reads hashboard detect GPIOs `439`, `440`, `441`
- toggles reset, sends the known BM1362 init frame, then sends the simple ping frame
- buffers UART data by reply frame boundary
- prints one complete 11-byte ASIC reply per line as hexadecimal
- counts total replies and unique reply patterns cleanly
- supports direct temperature reads from both onboard sensors on a selected hashboard:
  - HB0: `0x4C`, `0x48`
  - HB1: `0x4D`, `0x49`
  - HB2: `0x4E`, `0x4A`
- supports direct EEPROM reads on the same native Linux I2C bus:
  - HB0: `0x50`
  - HB1: `0x51`
  - HB2: `0x52`
- includes a native Rust port of the legacy Antminer v4 EEPROM decode path:
  - reads version from byte `0x00`
  - splits byte `0x01` into algorithm and key index
  - currently supports the observed `XXTEA` decode path used by these S19j Pro boards
  - decodes board identity and test-parameter fields directly in `hashboard_s19jpro`
  - reports PT1/PT2 CRC values from the decoded record

Current live behavior on the connected S19j Pro hashboards:

- all three hashboards report present on their detect GPIOs
- each board currently returns the repeated 11-byte reply:
  - `AA 55 13 62 03 00 00 00 00 00 1E`
- each board produced `126` framed replies during the current ping test
- each board produced `1` unique reply pattern during that test
- direct native I2C scan results on the live board:
  - `/dev/i2c-0` contains TMP75-class devices at `0x48`, `0x49`, `0x4A`, `0x4C`, `0x4D`, `0x4E`
  - `/dev/i2c-2` did not show the hashboard temperature sensors during this test
- current live temperature reads:
  - HB0: `39.0000 °C`, `33.3750 °C`
  - HB1: `39.1250 °C`, `33.1875 °C`
  - HB2: `41.1250 °C`, `34.3750 °C`
- current live EEPROM observations:
  - all three boards expose `256` bytes at `0x50`/`0x51`/`0x52`
  - byte `0x00` is `0x04` on all three boards
  - byte `0x01` is `0x11` on all three boards
  - bytes `0x02..0x61` are board-specific encoded payloads
  - bytes `0x70..0xFF` are mostly `0xFF` with a short board-specific prefix
  - byte `0xFF` is `0x5A` on all three boards
  - the native Rust decoder in this project successfully treats these as legacy Antminer EEPROM v4 records with algorithm/key byte `0x11`
  - decoded fields now printed directly by `hashboard_s19jpro eeprom <board>` look plausible on all three boards:
    - board name: `BHB42603`
    - factory job: `KPMI20220401001`
    - board serials: `KPMIYNRBBJDBH1985`, `KPMIYNRBBJDBI0792`, `KPMIYNRBBJDAE0701`
    - chip markings: `P1CH22AH42`, `J1CV22AH16`, `J1C022AD1C`
    - chip bins: `1`, `2`, `3`
    - frequency: `525 MHz`
    - voltage: `13.60-13.80 V`
    - nonce rate: `9989-9998`
    - PCB temperatures: `25-28 °C`
    - PT1 CRC matches on all three boards
    - PT2 CRC matches when calculated over the decoded PT2 region bytes
  - this is a better fit than the newer EVP-style interpretation previously assumed

Example:

- `/home/root/hashboard_s19jpro check`
- `/home/root/hashboard_s19jpro check 0`
- `/home/root/hashboard_s19jpro temps 0`
- `/home/root/hashboard_s19jpro temps 1`
- `/home/root/hashboard_s19jpro temps 2`
- `/home/root/hashboard_s19jpro eeprom 0`
- `/home/root/hashboard_s19jpro eeprom 1`
- `/home/root/hashboard_s19jpro eeprom 2`

Example summary output:

- `response_count=126`
- `unique_reply_count=1`
- `unique_reply 01: count=126 data=AA 55 13 62 03 00 00 00 00 00 1E`

Example temperature output:

- `temp0: address=0x4C raw=27 00 temp_c=39.0000`
- `temp1: address=0x48 raw=21 60 temp_c=33.3750`

Example EEPROM output:

- `board_info_version=0x04 (V4)`
- `algorithm_and_key_version=0x11 (algorithm=XXTEA key_index=1)`
- `board_serial=KPMIYNRBBJDBH1985`
- `board_name=BHB42603`
- `factory_job=KPMI20220401001`
- `chip_marking=P1CH22AH42`
- `voltage_v=13.80`
- `frequency_mhz=525`
- `nonce_rate=9989`
- `pt1_crc=stored:0x1C calculated:0x1C match:true`
- `pt2_crc=stored:0x15 tool_calc:0x12 region_calc:0x15`

## Build

Recommended target:

- `aarch64-unknown-linux-musl`

Example:

- `cargo build --release --target aarch64-unknown-linux-musl`

Build one binary explicitly:

- `cargo build --release --target aarch64-unknown-linux-musl --bin apw12-psu-tool`
- `cargo build --release --target aarch64-unknown-linux-musl --bin controlboard-misc`
- `cargo build --release --target aarch64-unknown-linux-musl --bin fan-tool`
- `cargo build --release --target aarch64-unknown-linux-musl --bin hashboard_s19jpro`

## Deploy

Example copy target:

- `/home/root/apw12-psu-tool`
- `/home/root/controlboard-misc`
- `/home/root/fan-tool`
- `/home/root/hashboard_s19jpro`

The corresponding compiled binaries will appear under:

- `target/<triple>/release/apw12-psu-tool`
- `target/<triple>/release/controlboard-misc`
- `target/<triple>/release/fan-tool`
- `target/<triple>/release/hashboard_s19jpro`

## Safety notes

- Disable the watchdog before longer experiments.
- Avoid `WRITE_CAL` until the calibration map is fully understood.
- Voltage changes should be followed by a short settling delay before measurement.
- Fan speed changes should also be followed by a short settling delay before reading RPM.
- `fan-tool` currently assumes 2 tach pulses per revolution and two shared PWM channels for four fans.

## Credits

- [Mujina](https://github.com/256foundation/mujina/) for being the best
- zbomzz for PSU protocol reverse engineering
- [Hashsource](https://github.com/HashSource/Antminer-APW12-Firmware) for PSU firmware dumps
- [Alex20129](https://github.com/Alex20129/eeprom_tool) for EEPROM decoding

## License

This project is licensed under the GNU General Public License v3.0 (GPLv3).
