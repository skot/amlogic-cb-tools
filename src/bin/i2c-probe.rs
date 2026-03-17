use amlogic_cb_tools::linux_i2c::LinuxI2cDevice;
use std::env;
use std::path::PathBuf;

const DEFAULT_I2C_DEVICE: &str = "/dev/i2c-0";

enum Command {
    Help,
    Scan { start: u16, end: u16 },
    Probe { address: u16 },
}

struct Config {
    i2c_device: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            i2c_device: PathBuf::from(DEFAULT_I2C_DEVICE),
        }
    }
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
        Command::Scan { start, end } => scan_bus(&config, start, end),
        Command::Probe { address } => probe_address(&config, address),
    }
    Ok(())
}

fn scan_bus(config: &Config, start: u16, end: u16) {
    let range = if start <= end { start..=end } else { end..=start };

    for address in range {
        print_probe_result(config, address);
    }
}

fn probe_address(config: &Config, address: u16) {
    print_probe_result(config, address);
}

fn print_probe_result(config: &Config, address: u16) {
    match LinuxI2cDevice::open(&config.i2c_device, address) {
        Ok(mut dev) => match dev.quick_write() {
            Ok(()) => println!("0x{address:02X}: ack quick-write"),
            Err(quick_err) => match dev.read_byte_transaction() {
                Ok(byte) => println!("0x{address:02X}: ack receive-byte=0x{byte:02X}"),
                Err(read_err) => println!(
                    "0x{address:02X}: no-ack-or-unsupported (quick={quick_err}; receive-byte={read_err})"
                ),
            },
        },
        Err(err) => println!("0x{address:02X}: open/select failed ({err})"),
    }
}

fn parse_args(args: Vec<String>) -> Result<(Config, Command), Box<dyn std::error::Error>> {
    let mut config = Config::default();
    let mut positionals = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--device" => {
                index += 1;
                config.i2c_device = PathBuf::from(args.get(index).ok_or("missing value for --device")?);
            }
            other => positionals.push(other.to_string()),
        }
        index += 1;
    }

    let command = match positionals.first().map(String::as_str) {
        None | Some("help") | Some("--help") | Some("-h") => Command::Help,
        Some("scan") => {
            let start = positionals.get(1).map(|s| parse_u16(s)).transpose()?.unwrap_or(0x03);
            let end = positionals.get(2).map(|s| parse_u16(s)).transpose()?.unwrap_or(0x77);
            Command::Scan { start, end }
        }
        Some("probe") => {
            let address = parse_u16(positionals.get(1).ok_or("missing address")?)?;
            Command::Probe { address }
        }
        Some(other) => return Err(format!("unknown command: {other}").into()),
    };

    Ok((config, command))
}

fn parse_u16(value: &str) -> Result<u16, Box<dyn std::error::Error>> {
    if let Some(stripped) = value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")) {
        Ok(u16::from_str_radix(stripped, 16)?)
    } else {
        Ok(value.parse()?)
    }
}

fn print_help() {
    println!("i2c-probe");
    println!();
    println!("Minimal Linux I2C receive-byte probe utility.");
    println!();
    println!("Global options:");
    println!("  --device <path>   Linux I2C device (default: {DEFAULT_I2C_DEVICE})");
    println!();
    println!("Commands:");
    println!("  help");
    println!("  scan [start] [end]");
    println!("  probe <address>");
    println!();
    println!("Examples:");
    println!("  i2c-probe --device /dev/i2c-2 scan 0x03 0x77");
    println!("  i2c-probe --device /dev/i2c-2 probe 0x49");
}
