use amlogic_cb_tools::gpio::SysfsGpio;
use amlogic_cb_tools::linux_i2c::LinuxI2cDevice;
use amlogic_cb_tools::protocol::{
    self, CMD_GET_FW_VERSION, CMD_GET_HW_VERSION, CMD_GET_VOLTAGE, CMD_MEASURE_VOLTAGE,
    CMD_READ_CAL, CMD_READ_STATE, CMD_SET_VOLTAGE, CMD_WATCHDOG, DEFAULT_PSU_ADDRESS,
    DEFAULT_PSU_WRITE_REGISTER, NAK_BYTE, PREAMBLE_LSB, PREAMBLE_MSB, build_frame,
    decode_dac_to_voltage, decode_measured_voltage, encode_voltage_to_dac, parse_frame,
};
use std::env;
use std::path::PathBuf;

const DEFAULT_I2C_DEVICE: &str = "/dev/i2c-1";
const DEFAULT_PSU_ENABLE_GPIO: u32 = 437;
const DEFAULT_SETTLE_SECONDS: u64 = 2;
const RESPONSE_DELAY_MILLIS: u64 = 500;
const MAX_RESPONSE_ATTEMPTS: usize = 3;

#[derive(Debug, Clone)]
struct Config {
    i2c_device: PathBuf,
    address: u16,
    write_register: u8,
    psu_enable_gpio: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            i2c_device: PathBuf::from(DEFAULT_I2C_DEVICE),
            address: DEFAULT_PSU_ADDRESS,
            write_register: DEFAULT_PSU_WRITE_REGISTER,
            psu_enable_gpio: DEFAULT_PSU_ENABLE_GPIO,
        }
    }
}

#[derive(Debug, Clone)]
enum Command {
    Help,
    PrepareBoard,
    OutputOn,
    OutputOff,
    Scan { start: u16, end: u16 },
    GetFw,
    GetHw,
    GetVoltage,
    MeasureVoltage,
    ReadState,
    DisableWatchdog,
    EnableWatchdog(u8),
    SetDac(u8),
    SetVoltage(f32),
    ReadCal { page: u8, count: u8 },
    Raw { command: u8, payload: Vec<u8> },
}

#[derive(Debug, Clone)]
struct SetVoltageOutcome {
    frame: protocol::Frame,
    verified_by_readback: bool,
    response_issue: Option<String>,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let (config, command) = parse_args(env::args().skip(1).collect())?;

    match command {
        Command::Help => print_help(),
        Command::PrepareBoard => {
            let gpio = SysfsGpio::new(config.psu_enable_gpio);
            gpio.set_output_high()?;
            println!(
                "PSU enable GPIO {} exported and driven high (output disabled; active-low enable)",
                config.psu_enable_gpio
            );
        }
        Command::OutputOn => {
            let gpio = SysfsGpio::new(config.psu_enable_gpio);
            gpio.set_output_low()?;
            println!(
                "PSU enable GPIO {} driven low (output enabled; active-low)",
                config.psu_enable_gpio
            );
        }
        Command::OutputOff => {
            let gpio = SysfsGpio::new(config.psu_enable_gpio);
            gpio.set_output_high()?;
            println!(
                "PSU enable GPIO {} driven high (output disabled; active-low)",
                config.psu_enable_gpio
            );
        }
        Command::Scan { start, end } => scan_bus(&config, start, end)?,
        other => {
            let mut dev = LinuxI2cDevice::open(&config.i2c_device, config.address)?;
            run_i2c_command(&config, &mut dev, other)?;
        }
    }

    Ok(())
}

fn scan_bus(config: &Config, start: u16, end: u16) -> Result<(), Box<dyn std::error::Error>> {
    let range = if start <= end { start..=end } else { end..=start };

    for address in range {
        match LinuxI2cDevice::open(&config.i2c_device, address) {
            Ok(mut dev) => match exchange(config, &mut dev, CMD_GET_FW_VERSION, &[]) {
                Ok(frame) => {
                    println!(
                        "0x{address:02X}: APW-like response, fw={}",
                        String::from_utf8_lossy(&frame.payload)
                    );
                }
                Err(err) => {
                    println!("0x{address:02X}: no APW response ({err})");
                }
            },
            Err(err) => println!("0x{address:02X}: bus open/select failed ({err})"),
        }
    }

    Ok(())
}

fn run_i2c_command(
    config: &Config,
    dev: &mut LinuxI2cDevice,
    command: Command,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Command::Help
        | Command::PrepareBoard
        | Command::OutputOn
        | Command::OutputOff
        | Command::Scan { .. } => {}
        Command::GetFw => {
            let frame = exchange(config, dev, CMD_GET_FW_VERSION, &[])?;
            println!("firmware payload: {:02X?}", frame.payload);
            println!("raw: {:02X?}", frame.raw);
        }
        Command::GetHw => {
            let frame = exchange(config, dev, CMD_GET_HW_VERSION, &[])?;
            println!("hardware payload: {:02X?}", frame.payload);
            println!("raw: {:02X?}", frame.raw);
        }
        Command::GetVoltage => {
            let frame = exchange(config, dev, CMD_GET_VOLTAGE, &[])?;
            let dac = *frame.payload.first().ok_or("missing DAC payload")?;
            println!("dac_code=0x{dac:02X} ({dac})");
            println!("estimated_voltage={:.4} V", decode_dac_to_voltage(dac));
            println!("raw: {:02X?}", frame.raw);
        }
        Command::MeasureVoltage => {
            let frame = exchange(config, dev, CMD_MEASURE_VOLTAGE, &[])?;
            if frame.payload.len() < 2 {
                return Err("missing ADC payload".into());
            }
            let volts = decode_measured_voltage(frame.payload[0], frame.payload[1]);
            println!(
                "adc_raw=0x{:02X}{:02X} measured_voltage={:.4} V",
                frame.payload[1], frame.payload[0], volts
            );
            println!("raw: {:02X?}", frame.raw);
        }
        Command::ReadState => {
            let frame = exchange(config, dev, CMD_READ_STATE, &[])?;
            if frame.payload.len() < 2 {
                return Err("missing state payload".into());
            }
            let state = u16::from(frame.payload[0]) | (u16::from(frame.payload[1]) << 8);
            let label = if state == 1 { "ON" } else { "OFF" };
            println!("state=0x{state:04X} ({label})");
            println!("raw: {:02X?}", frame.raw);
        }
        Command::DisableWatchdog => {
            let frame = exchange(config, dev, CMD_WATCHDOG, &[0x00, 0x00])?;
            println!("watchdog disabled");
            println!("raw: {:02X?}", frame.raw);
        }
        Command::EnableWatchdog(value) => {
            let frame = exchange(config, dev, CMD_WATCHDOG, &[value, 0x00])?;
            println!("watchdog enabled with payload 0x{value:02X}");
            println!("raw: {:02X?}", frame.raw);
        }
        Command::SetDac(dac) => {
            let outcome = set_voltage_command(config, dev, dac)?;
            println!("set dac=0x{dac:02X} ({dac}), target≈{:.4} V", decode_dac_to_voltage(dac));
            if outcome.verified_by_readback {
                println!(
                    "status=accepted-by-readback (transient response issue while PSU applied new setpoint)"
                );
                if let Some(issue) = &outcome.response_issue {
                    println!("response_issue={issue}");
                }
            } else {
                println!("status=accepted-by-echo");
            }
            println!("raw: {:02X?}", outcome.frame.raw);
        }
        Command::SetVoltage(volts) => {
            let dac = encode_voltage_to_dac(volts);
            let outcome = set_voltage_command(config, dev, dac)?;
            println!(
                "requested_voltage={:.4} V -> dac=0x{dac:02X} ({dac}) -> fit_voltage={:.4} V",
                volts,
                decode_dac_to_voltage(dac)
            );
            if outcome.verified_by_readback {
                println!(
                    "status=accepted-by-readback (transient response issue while PSU applied new setpoint)"
                );
                if let Some(issue) = &outcome.response_issue {
                    println!("response_issue={issue}");
                }
            } else {
                println!("status=accepted-by-echo");
            }
            println!("raw: {:02X?}", outcome.frame.raw);
            std::thread::sleep(std::time::Duration::from_secs(DEFAULT_SETTLE_SECONDS));
            match exchange(config, dev, CMD_MEASURE_VOLTAGE, &[]) {
                Ok(verify) if verify.payload.len() >= 2 => {
                    let measured = decode_measured_voltage(verify.payload[0], verify.payload[1]);
                    println!("measured_voltage_after_set={:.4} V", measured);
                }
                Ok(_) => {
                    println!("warning: post-set voltage measurement reply was too short");
                }
                Err(err) => {
                    println!("warning: post-set voltage measurement failed: {err}");
                }
            }
        }
        Command::ReadCal { page, count } => {
            let frame = exchange(config, dev, CMD_READ_CAL, &[page, count])?;
            println!("calibration_page=0x{page:02X} bytes={:02X?}", frame.payload);
            println!("raw: {:02X?}", frame.raw);
        }
        Command::Raw { command, payload } => {
            let frame = exchange(config, dev, command, &payload)?;
            println!("response command=0x{:02X}", frame.command);
            println!("payload={:02X?}", frame.payload);
            println!("raw: {:02X?}", frame.raw);
        }
    }

    println!(
        "device={} address=0x{:02X} register=0x{:02X}",
        config.i2c_device.display(),
        config.address,
        config.write_register,
    );
    Ok(())
}

fn exchange(
    config: &Config,
    dev: &mut LinuxI2cDevice,
    command: u8,
    payload: &[u8],
) -> Result<protocol::Frame, Box<dyn std::error::Error>> {
    let frame = build_frame(command, payload);
    for byte in frame {
        dev.write_byte_transaction(config.write_register, byte)?;
    }

    std::thread::sleep(std::time::Duration::from_millis(RESPONSE_DELAY_MILLIS));

    let mut last_error: Option<String> = None;
    for _ in 0..MAX_RESPONSE_ATTEMPTS {
        let response = read_response_frame(dev)?;
        if response == [NAK_BYTE] {
            last_error = Some("PSU returned NAK (0xF5)".to_string());
            std::thread::sleep(std::time::Duration::from_millis(RESPONSE_DELAY_MILLIS));
            continue;
        }

        match parse_frame(&response) {
            Ok(frame) if frame.command == command => return Ok(frame),
            Ok(frame) => {
                last_error = Some(format!(
                    "unexpected response command 0x{:02X} for request 0x{command:02X}",
                    frame.command
                ));
            }
            Err(err) => {
                last_error = Some(err.to_string());
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(RESPONSE_DELAY_MILLIS));
    }

    Err(last_error
        .unwrap_or_else(|| "no valid PSU response received".to_string())
        .into())
}

fn set_voltage_command(
    config: &Config,
    dev: &mut LinuxI2cDevice,
    dac: u8,
) -> Result<SetVoltageOutcome, Box<dyn std::error::Error>> {
    match exchange(config, dev, CMD_SET_VOLTAGE, &[dac, 0x00]) {
        Ok(frame) => Ok(SetVoltageOutcome {
            frame,
            verified_by_readback: false,
            response_issue: None,
        }),
        Err(err) => {
            let verify = exchange(config, dev, CMD_GET_VOLTAGE, &[])?;
            let readback = *verify
                .payload
                .first()
                .ok_or("missing DAC payload during set verification")?;
            if readback == dac {
                Ok(SetVoltageOutcome {
                    frame: protocol::Frame {
                        command: CMD_SET_VOLTAGE,
                        payload: vec![dac, 0x00],
                        raw: verify.raw,
                    },
                    verified_by_readback: true,
                    response_issue: Some(err.to_string()),
                })
            } else {
                Err(err)
            }
        }
    }
}

fn read_response_frame(dev: &mut LinuxI2cDevice) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut first = dev.read_byte_transaction()?;
    while first != PREAMBLE_LSB && first != NAK_BYTE {
        first = dev.read_byte_transaction()?;
    }

    if first == NAK_BYTE {
        return Ok(vec![NAK_BYTE]);
    }

    let second = dev.read_byte_transaction()?;
    if second != PREAMBLE_MSB {
        return Err(format!("invalid preamble continuation: 0x{second:02X}").into());
    }

    let length = dev.read_byte_transaction()?;
    let mut response = Vec::with_capacity(usize::from(length) + 2);
    response.push(first);
    response.push(second);
    response.push(length);

    let remaining = usize::from(length)
        .checked_sub(1)
        .ok_or("response length underflow")?;
    for _ in 0..remaining {
        response.push(dev.read_byte_transaction()?);
    }

    Ok(response)
}

fn parse_args(args: Vec<String>) -> Result<(Config, Command), Box<dyn std::error::Error>> {
    let mut config = Config::default();
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--device" => {
                i += 1;
                config.i2c_device = PathBuf::from(args.get(i).ok_or("missing value for --device")?);
            }
            "--address" => {
                i += 1;
                config.address = parse_u16(args.get(i).ok_or("missing value for --address")?)?;
            }
            "--psu-enable-gpio" => {
                i += 1;
                config.psu_enable_gpio = args
                    .get(i)
                    .ok_or("missing value for --psu-enable-gpio")?
                    .parse()?;
            }
            "--register" => {
                i += 1;
                config.write_register = parse_u8(args.get(i).ok_or("missing value for --register")?)?;
            }
            other => positionals.push(other.to_string()),
        }
        i += 1;
    }

    let command = match positionals.first().map(String::as_str) {
        None | Some("help") | Some("--help") | Some("-h") => Command::Help,
        Some("prepare-board") => Command::PrepareBoard,
        Some("output-on") => Command::OutputOn,
        Some("output-off") => Command::OutputOff,
        Some("scan") => {
            let start = positionals.get(1).map(|s| parse_u16(s)).transpose()?.unwrap_or(0x50);
            let end = positionals.get(2).map(|s| parse_u16(s)).transpose()?.unwrap_or(0x5F);
            Command::Scan { start, end }
        }
        Some("get-fw") => Command::GetFw,
        Some("get-hw") => Command::GetHw,
        Some("get-voltage") => Command::GetVoltage,
        Some("measure-voltage") => Command::MeasureVoltage,
        Some("read-state") => Command::ReadState,
        Some("disable-watchdog") => Command::DisableWatchdog,
        Some("enable-watchdog") => {
            let value = positionals.get(1).map(|s| parse_u8(s)).transpose()?.unwrap_or(0x0E);
            Command::EnableWatchdog(value)
        }
        Some("set-dac") => Command::SetDac(parse_u8(positionals.get(1).ok_or("missing DAC value")?)?),
        Some("set-voltage") => {
            let volts: f32 = positionals.get(1).ok_or("missing target voltage")?.parse()?;
            Command::SetVoltage(volts)
        }
        Some("read-cal") => {
            let page = parse_u8(positionals.get(1).ok_or("missing page value")?)?;
            let count = parse_u8(positionals.get(2).ok_or("missing count value")?)?;
            Command::ReadCal { page, count }
        }
        Some("raw") => {
            let command = parse_u8(positionals.get(1).ok_or("missing raw command byte")?)?;
            let payload = positionals
                .iter()
                .skip(2)
                .map(|value| parse_u8(value))
                .collect::<Result<Vec<_>, _>>()?;
            Command::Raw { command, payload }
        }
        Some(other) => return Err(format!("unknown command: {other}").into()),
    };

    Ok((config, command))
}

fn parse_u8(value: &str) -> Result<u8, Box<dyn std::error::Error>> {
    if let Some(stripped) = value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")) {
        Ok(u8::from_str_radix(stripped, 16)?)
    } else {
        Ok(value.parse()?)
    }
}

fn parse_u16(value: &str) -> Result<u16, Box<dyn std::error::Error>> {
    if let Some(stripped) = value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")) {
        Ok(u16::from_str_radix(stripped, 16)?)
    } else {
        Ok(value.parse()?)
    }
}

fn print_help() {
    println!("apw12-psu-tool");
    println!();
    println!("Standalone APW12 PSU control utility for Amlogic S19j Pro control boards.");
    println!();
    println!("Global options:");
    println!("  --device <path>            Linux I2C device (default: {DEFAULT_I2C_DEVICE})");
    println!("  --address <addr>           live LuxOS-facing I2C address in hex or decimal (default: 0x10)");
    println!("  --register <reg>           live LuxOS-facing outbound register byte (default: 0x11)");
    println!("  --psu-enable-gpio <n>      PSU enable GPIO for prepare-board (default: 437)");
    println!();
    println!("Commands:");
    println!("  help");
    println!("  prepare-board            Export GPIO and leave PSU output disabled");
    println!("  output-on                Drive active-low PSU enable GPIO low");
    println!("  output-off               Drive active-low PSU enable GPIO high");
    println!("  scan [start] [end]");
    println!("  get-fw");
    println!("  get-hw");
    println!("  get-voltage");
    println!("  measure-voltage");
    println!("  read-state");
    println!("  disable-watchdog");
    println!("  enable-watchdog [value]");
    println!("  set-dac <dac>");
    println!("  set-voltage <volts>");
    println!("  read-cal <page> <count>");
    println!("  raw <cmd> [payload bytes...]");
    println!();
    println!("Examples:");
    println!("  apw12-psu-tool prepare-board");
    println!("  apw12-psu-tool scan");
    println!("  apw12-psu-tool disable-watchdog");
    println!("  apw12-psu-tool set-voltage 12.6");
    println!("  apw12-psu-tool read-cal 0x40 0x21");
}