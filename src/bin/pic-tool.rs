//! Antminer hashboard PIC microcontroller tool.
//!
//! CLI wrapper around `amlogic_cb_tools::pic` for the on-hashboard
//! PIC microcontroller (PIC16F1704) found on PIC-variant Antminer
//! hashboards (BHB42601 / S19j Pro and family). The PIC gates the
//! per-domain DC-DC regulators, holds calibration, and proxies
//! temperature sensors.
//!
//! See `src/pic.rs` for protocol details. The PIC requires PSU output
//! to be ON to respond (its onboard LDO is fed from the 12 V rail).

use std::env;
use std::path::PathBuf;

use amlogic_cb_tools::pic::{DEFAULT_I2C_DEVICE, PicChain};

struct PicConfig {
    device: PathBuf,
    address: u16,
    confirm_state_change: bool,
    verbose: bool,
}

fn open(cfg: &PicConfig) -> Result<PicChain, String> {
    if cfg.verbose {
        println!(
            "open device={} address=0x{:02x}",
            cfg.device.display(),
            cfg.address
        );
    }
    PicChain::open(&cfg.device, cfg.address).map_err(|e| e.to_string())
}

fn require_state_change(cfg: &PicConfig, name: &str) -> Result<(), String> {
    if cfg.confirm_state_change {
        Ok(())
    } else {
        Err(format!(
            "command '{name}' changes PIC state; rerun with --confirm-state-change"
        ))
    }
}

fn cmd_version(cfg: &PicConfig) -> Result<(), String> {
    let mut pic = open(cfg)?;
    let v = pic.get_sw_version().map_err(|e| e.to_string())?;
    println!("pic_version: 0x{v:02x} at addr 0x{:02x}", cfg.address);
    Ok(())
}

fn cmd_heartbeat(cfg: &PicConfig) -> Result<(), String> {
    let mut pic = open(cfg)?;
    pic.heartbeat().map_err(|e| e.to_string())?;
    println!("heartbeat ok at 0x{:02x}", cfg.address);
    Ok(())
}

fn cmd_reset(cfg: &PicConfig) -> Result<(), String> {
    require_state_change(cfg, "reset")?;
    let mut pic = open(cfg)?;
    pic.reset().map_err(|e| e.to_string())?;
    println!("reset ok at 0x{:02x}", cfg.address);
    Ok(())
}

fn cmd_start_app(cfg: &PicConfig) -> Result<(), String> {
    require_state_change(cfg, "start-app")?;
    let mut pic = open(cfg)?;
    pic.start_app().map_err(|e| e.to_string())?;
    println!("start-app ok at 0x{:02x}", cfg.address);
    Ok(())
}

fn cmd_set_dc_dc(cfg: &PicConfig, enable: bool) -> Result<(), String> {
    require_state_change(
        cfg,
        if enable {
            "enable-dc-dc"
        } else {
            "disable-dc-dc"
        },
    )?;
    let mut pic = open(cfg)?;
    if enable {
        pic.enable_dc_dc().map_err(|e| e.to_string())?;
        println!("dc-dc enabled at 0x{:02x}", cfg.address);
    } else {
        pic.disable_dc_dc().map_err(|e| e.to_string())?;
        println!("dc-dc disabled at 0x{:02x}", cfg.address);
    }
    Ok(())
}

fn cmd_handshake(cfg: &PicConfig) -> Result<(), String> {
    require_state_change(cfg, "handshake")?;
    let mut pic = open(cfg)?;
    let v = pic.handshake().map_err(|e| e.to_string())?;
    println!(
        "handshake ok at 0x{:02x}: pic_version=0x{v:02x}, dc-dc disabled",
        cfg.address
    );
    Ok(())
}

fn cmd_read_reg(cfg: &PicConfig, reg: u8) -> Result<(), String> {
    let mut pic = open(cfg)?;
    let v = pic.read_reg(reg).map_err(|e| e.to_string())?;
    println!("read_reg 0x{reg:02x} = 0x{v:04x} ({v})");
    Ok(())
}

fn print_help() {
    eprintln!(
        "pic-tool

Antminer hashboard PIC microcontroller utility.

NOTE: The PIC requires PSU output to be ON (12 V on the rail) to
respond. Use apw12-psu-tool prepare-board + set-voltage + output-on
first.

Global options:
  --device <path>             Linux I2C device (default: {DEFAULT_I2C_DEVICE})
  --address <addr>            PIC I2C address in hex or decimal
                              (HB0=0x20, HB1=0x21, HB2=0x22)
  --confirm-state-change      Required to run state-changing commands
  --verbose                   Print extra debug info

Read commands (PIC must be in app mode for read_reg):
  version                     Read PIC firmware version
  heartbeat                   Send heartbeat ping
  read-reg <reg>              Read 16-bit register (e.g. 0x48..0x4b for temps)

State-changing commands (require --confirm-state-change):
  reset                       Reset PIC (returns to bootloader)
  start-app                   Bootloader -> application jump
  enable-dc-dc                Enable per-domain DC-DC regulators
                              (DANGEROUS without proper PSU/cooling setup)
  disable-dc-dc               Disable per-domain DC-DC regulators
  handshake                   reset + start-app + version + disable-dc-dc
                              (full init, leaves chips unpowered)

Examples:
  pic-tool --address 0x20 version
  pic-tool --address 0x20 --confirm-state-change handshake
  pic-tool --address 0x20 --confirm-state-change enable-dc-dc
  pic-tool --address 0x20 read-reg 0x48"
    );
}

fn parse_addr(s: &str) -> Result<u16, String> {
    if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u16::from_str_radix(rest, 16).map_err(|e| format!("bad hex address '{s}': {e}"))
    } else {
        s.parse::<u16>()
            .map_err(|e| format!("bad address '{s}': {e}"))
    }
}

fn parse_u8(s: &str) -> Result<u8, String> {
    if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u8::from_str_radix(rest, 16).map_err(|e| format!("bad hex byte '{s}': {e}"))
    } else {
        s.parse::<u8>().map_err(|e| format!("bad byte '{s}': {e}"))
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut cfg = PicConfig {
        device: PathBuf::from(DEFAULT_I2C_DEVICE),
        address: 0x20,
        confirm_state_change: false,
        verbose: false,
    };

    let mut command: Option<String> = None;
    let mut positional: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "--device" => {
                cfg.device = PathBuf::from(args.get(i + 1).cloned().unwrap_or_default());
                i += 2;
            }
            "--address" => {
                let v = args.get(i + 1).cloned().unwrap_or_default();
                cfg.address = match parse_addr(&v) {
                    Ok(a) => a,
                    Err(e) => {
                        eprintln!("error: {e}");
                        std::process::exit(2);
                    }
                };
                i += 2;
            }
            "--confirm-state-change" => {
                cfg.confirm_state_change = true;
                i += 1;
            }
            "--verbose" => {
                cfg.verbose = true;
                i += 1;
            }
            "--help" | "-h" | "help" => {
                print_help();
                std::process::exit(0);
            }
            other if !other.starts_with("--") && command.is_none() => {
                command = Some(other.to_string());
                i += 1;
            }
            other if !other.starts_with("--") => {
                positional.push(other.to_string());
                i += 1;
            }
            _ => {
                eprintln!("error: unknown arg '{a}'");
                print_help();
                std::process::exit(2);
            }
        }
    }

    let Some(cmd) = command else {
        print_help();
        std::process::exit(2);
    };

    let res = match cmd.as_str() {
        "version" => cmd_version(&cfg),
        "heartbeat" => cmd_heartbeat(&cfg),
        "reset" => cmd_reset(&cfg),
        "start-app" => cmd_start_app(&cfg),
        "enable-dc-dc" => cmd_set_dc_dc(&cfg, true),
        "disable-dc-dc" => cmd_set_dc_dc(&cfg, false),
        "handshake" => cmd_handshake(&cfg),
        "read-reg" => match positional.first().map(String::as_str).map(parse_u8) {
            Some(Ok(reg)) => cmd_read_reg(&cfg, reg),
            Some(Err(e)) => Err(e),
            None => Err("read-reg requires a register address (e.g. 0x48)".into()),
        },
        other => {
            eprintln!("error: unknown command '{other}'");
            print_help();
            std::process::exit(2);
        }
    };

    if let Err(e) = res {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
