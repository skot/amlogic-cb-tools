# amlogic control-board tools

![Alt text](doc/amlogicA113D.png)

Tools and notes for using the Amlogic A113D processor as featured on the Bitmain Antminer controlboards

## Purpose

This project is intentionally separate from [Mujina](https://github.com/256foundation/mujina/). It is meant to host small standalone binaries that can be deployed onto a live control board so hardware behavior can be validated independently before integrating support into Mujina.

For a consolidated explanation of the observed firmware and software stack on
the board, see [doc/boot-sequence.md](doc/boot-sequence.md).

## Binaries

- `apw12-psu-tool` — APW12 PSU control and telemetry
- `controlboard-misc` — misc GPIO helpers for control-board LEDs
- `fan-tool` — fan PWM and tachometer experiments for the Amlogic control board
- `hashboard_s19jpro` — direct serial and GPIO sanity checks for the three connected S19j Pro hashboards
- `oled-ssd1306` — simple SSD1306 OLED text output over the confirmed `IIC3` Linux I2C bus

## LuxOS firmware base
LuxOS makes a good base firmware for experimentation. When flashed to an Amlogic controlboard you can ssh to the controlboard with user root/root and the whole filesystem is writeable. We're not interested in running LuxOS though, so you can disable the luxminer app as follows;

Before running the tools on a live LuxOS control board, disable `luxminer` and
`luxupdate` so they do not interfere with PSU control, fan control, or other
hardware access.

This project includes a helper script:

- [disable_luxminer.exp](disable_luxminer.exp)

Usage:

- `chmod +x disable_luxminer.exp`
- `./disable_luxminer.exp <board-ip>`

Example:

- `./disable_luxminer.exp <controlboard_ip>`

What it does:

- kills any running `luxminer` / `luxupdate` processes
- replaces `/luxminer` and `/luxupdate` with inert symlinks to `/bin/false`
- preserves the original binaries as backup files
- replaces the LuxOS init script with a no-op stub
- disables the LuxOS runlevel startup links

The original LuxOS binaries are preserved and not deleted.

After disabling LuxOS mining tools, deploy and run the binaries from this
project.

## Board connections

This section centralizes the known Linux-visible interfaces on the Amlogic
control board.

### PSU control path

- dedicated PSU I2C bus: `/dev/i2c-1`
- default APW12 transport address used by this project: `0x10`
- default outbound byte-write register used by this project: `0x11`
- PSU enable GPIO: `437`
- current software model: framed APW12 packets sent and received one byte per
  I2C transaction on the dedicated PSU bus

### Fan control and tach inputs

- PWM control paths:
  - `/sys/class/pwm/pwmchip0/pwm0`
  - `/sys/class/pwm/pwmchip0/pwm1`
- current PWM assumption used by `fan-tool`: 100 kHz period (`10000` ns)
- tachometer GPIO inputs:
  - FAN0: `447` (`GPIOA_10`)
  - FAN1: `448` (`GPIOA_11`)
  - FAN2: `449` (`GPIOA_12`)
  - FAN3: `450` (`GPIOA_13`)
- current RPM measurement approach: Linux sysfs GPIO polling on falling edges

### Misc control-board GPIO

- green LED GPIO: `453` (`GPIOA_16`)
- red LED GPIO: `438` (`GPIOA_1`)
- IP Report Button: `445` (`GPIOA_8`)

### IIC3 test points

- `IIC3_SCL` test point: `GPIOZ_8`
- `IIC3_SDA` test point: `GPIOZ_9`
- Linux I2C adapter: `/dev/i2c-2`
- current OLED experiment target: SSD1306 `128x32` display, typically at `0x3C`

### Hashboard serial links

- HB0 UART: `/dev/ttyS3` on `GPIOAO_4`/`GPIOAO_5` (`501`/`502`, `uart_ao_b` TX/RX)
- HB1 UART: `/dev/ttyS2` on `GPIOZ_2`/`GPIOZ_3` (`413`/`414`, `uart_b` TX/RX)
- HB2 UART: `/dev/ttyS1` on `GPIOX_8`/`GPIOX_9` (`466`/`467`, `uart_a` TX/RX)

### Hashboard control GPIO

- reset GPIOs:
  - HB0: `454` (`GPIOA_17`)
  - HB1: `455` (`GPIOA_18`)
  - HB2: `456` (`GPIOA_19`)
- detect GPIOs:
  - HB0: `439` (`GPIOA_2`)
  - HB1: `440` (`GPIOA_3`)
  - HB2: `441` (`GPIOA_4`)

### Hashboard sensor and EEPROM bus

- native Linux I2C bus: `/dev/i2c-0`
- bus pins: `GPIOAO_10`/`GPIOAO_11` (`507`/`508`, `i2c_ao` SCL/SDA)
- TMP75 temperature sensor addresses:
  - HB0: `0x48`, `0x4C`
  - HB1: `0x4D`, `0x49`
  - HB2: `0x4E`, `0x4A`
- EEPROM addresses:
  - HB0: `0x50`
  - HB1: `0x51`
  - HB2: `0x52`


## Current apw12-psu-tool scope

- supports basic protocol commands such as:
  - address scanning on the dedicated PSU bus
  - firmware/hardware version
  - watchdog enable/disable
  - DAC setpoint readback
  - measured voltage readback
  - state readback
  - calibration reads
- includes a `prepare-board` helper that asserts PSU enable before talking to
  the APW12

Shared code lives in the library crate under `src/lib.rs`, with each executable in `src/bin/`.

## Current controlboard-misc scope

- controls the two misc control-board LEDs via sysfs GPIO
- reports the IP report button input state via sysfs GPIO
- supports these commands:
  - `controlboard-misc status`
  - `controlboard-misc set <green|red|all> <on|off|high|low|1|0>`
  - `controlboard-misc toggle [green|red|all]`

Example:

- `./controlboard-misc status`
- `./controlboard-misc set green on`
- `./controlboard-misc toggle red`
- `./controlboard-misc toggle all`

## Current fan-tool scope

- controls the board fan PWM outputs and reads tachometer inputs
- measures RPM by counting tach pulses over a sampling window
- supports these commands:
  - `fan-tool get-pwm`
  - `fan-tool set-percent <percent>`
  - `fan-tool set-pwm <all|0|1> <percent>`
  - `fan-tool read-rpm [all|0|1|2|3] [window_ms]`

Examples:

- `./fan-tool set-percent 15`
- `./fan-tool read-rpm all 1500`

## Current hashboard_s19jpro scope

- supports these commands:
  - `hashboard_s19jpro check [0|1|2]`
    - with no index, runs the serial sanity check against all three hashboards
    - with an index, targets one hashboard
    - toggles reset, sends the known BM1362 init frame, then sends the simple ping frame
    - buffers UART data by reply frame boundary
    - prints one complete 11-byte ASIC reply per line as hexadecimal
    - counts total replies and unique reply patterns cleanly
    - extracts the ASIC model from reply bytes 3 and 4 and summarizes detected models
  - `hashboard_s19jpro temps <0|1|2>`
    - reads both onboard TMP75 temperature sensors on the selected hashboard
  - `hashboard_s19jpro eeprom <0|1|2>`
    - reads the selected hashboard EEPROM on the native Linux I2C bus
    - includes a native Rust port of the legacy Antminer v4 EEPROM decode path:
      - reads version from byte `0x00`
      - splits byte `0x01` into algorithm and key index
      - currently supports the observed `XXTEA` decode path used by these S19j Pro boards
      - decodes board identity and test-parameter fields directly in `hashboard_s19jpro`
      - reports PT1/PT2 CRC values from the decoded record

Examples:

- `./hashboard_s19jpro check`
- `./hashboard_s19jpro check 1`
- `./hashboard_s19jpro temps 2`
- `./hashboard_s19jpro eeprom 0`

## Current oled-ssd1306 scope

- initializes a `128x32` SSD1306 OLED on `/dev/i2c-2`
- defaults to I2C address `0x3C`
- uses the `embedded-graphics` text stack with SSD1306 driver support
- clears the display and renders up to three centered text lines
- defaults to `Hello world` when no text is provided

Examples:

- `./oled-ssd1306`
- `./oled-ssd1306 "Hello world"`
- `./oled-ssd1306 "Hash OK" "Fan 4200"`
- `./oled-ssd1306 "HB0 OK" "PSU 12.3V" "IP 192.168.1.236"`

## Build

Recommended target:

- `aarch64-unknown-linux-musl`

For normal stock LuxOS systems, these tools should be built as `aarch64`
binaries.

This repository includes a Cargo target config that uses `rust-lld` for
`aarch64-unknown-linux-musl`, so the build commands below should work on macOS
without setting a separate linker environment variable each time.

Example:

- `cargo build --release --target aarch64-unknown-linux-musl`

Build one binary explicitly:

- `cargo build --release --target aarch64-unknown-linux-musl --bin apw12-psu-tool`
- `cargo build --release --target aarch64-unknown-linux-musl --bin controlboard-misc`
- `cargo build --release --target aarch64-unknown-linux-musl --bin fan-tool`
- `cargo build --release --target aarch64-unknown-linux-musl --bin hashboard_s19jpro`

The resulting binaries on the local build machine will appear under:

- `target/aarch64-unknown-linux-musl/release/apw12-psu-tool`
- `target/aarch64-unknown-linux-musl/release/controlboard-misc`
- `target/aarch64-unknown-linux-musl/release/fan-tool`
- `target/aarch64-unknown-linux-musl/release/hashboard_s19jpro`

### Note on the experimental USB-kernel branch

During the USB mass-storage / Wi-Fi kernel work, we tested a custom rebuilt
kernel and userspace combination that did not behave like a stock LuxOS board.
On that modified setup, the `aarch64` `amlogic-cb-tools` binaries were no
longer the right fit, and an `armv7-unknown-linux-gnueabihf` build produced
working binaries instead.

That behavior should be treated as specific to that experimental environment,
not as the default expectation for ordinary LuxOS systems.

If you are working on that modified USB-kernel setup, the host-side flow on
macOS that worked for us was:

- install `zig`
- install `cargo-zigbuild`
- install the Rust target:
  - `rustup target add armv7-unknown-linux-gnueabihf`

- `cargo zigbuild --release --target armv7-unknown-linux-gnueabihf`

- `target/armv7-unknown-linux-gnueabihf/release/<binary-name>`

## Deploy

The Amlogic control board OS exposes SSH, but it does not provide an SFTP
server. That means modern `scp` defaults may fail unless legacy SCP protocol is
forced explicitly.

Typical board details:

- IP: `<controlboard_ip>`
- user: `root`
- password: `root`

Recommended deployment flow:

1. Build the target binary for the board:
  - `cargo build --release --target aarch64-unknown-linux-musl --bin controlboard-misc`
2. Copy it with legacy SCP mode enabled:
  - `scp -O target/aarch64-unknown-linux-musl/release/controlboard-misc root@<controlboard_ip>:/home/root/controlboard-misc`
3. Log in over SSH:
  - `ssh root@<controlboard_ip>`
4. Mark it executable if needed:
  - `chmod +x /home/root/controlboard-misc`
5. Run it directly on the board:
  - `/home/root/controlboard-misc status`

If `scp` is attempted without `-O`, OpenSSH may try to use SFTP and fail with
an error similar to:

- `sh: line 1: /usr/libexec/sftp-server: No such file or directory`
- `scp: Connection closed`

Example copy target:

- `/home/root/apw12-psu-tool`
- `/home/root/controlboard-misc`
- `/home/root/fan-tool`
- `/home/root/hashboard_s19jpro`

If you prefer to avoid `scp -O`, the live LuxOS board also accepts a simple SSH
streaming copy:

- `ssh root@<controlboard_ip> "cat > /home/root/controlboard-misc && chmod 755 /home/root/controlboard-misc" < target/aarch64-unknown-linux-musl/release/controlboard-misc`



## Credits

- [Mujina](https://github.com/256foundation/mujina/) for being the best
- zbomzz for PSU protocol reverse engineering
- [Hashsource](https://github.com/HashSource/Antminer-APW12-Firmware) for PSU firmware dumps
- [Hashsource](https://github.com/HashSource/Amlogic_Guides) for rare Amlogic documentation
- [Alex20129](https://github.com/Alex20129/eeprom_tool) for EEPROM decoding

## License

This project is licensed under the GNU General Public License v3.0 (GPLv3).
