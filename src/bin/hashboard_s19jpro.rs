use amlogic_cb_tools::eeprom_antminer::{decode_antminer_eeprom, AntminerEepromVersion};
use amlogic_cb_tools::gpio::SysfsGpio;
use amlogic_cb_tools::linux_i2c::LinuxI2cDevice;
use amlogic_cb_tools::serial::LinuxSerialPort;
use std::collections::BTreeMap;
use std::env;
use std::thread;
use std::time::Duration;

const INIT_FRAME: [u8; 11] = [0x55, 0xAA, 0x51, 0x09, 0x00, 0xA4, 0x90, 0x00, 0xFF, 0xFF, 0x1C];
const PING_FRAME: [u8; 7] = [0x55, 0xAA, 0x52, 0x05, 0x00, 0x00, 0x0A];
const REPLY_PREAMBLE: [u8; 2] = [0xAA, 0x55];
const TMP75_I2C_DEVICE: &str = "/dev/i2c-0";
const TMP75_TEMP_REG: u8 = 0x00;
const EEPROM_I2C_DEVICE: &str = "/dev/i2c-0";
const SERIAL_BAUD: u32 = 115_200;
const SERIAL_TIMEOUT_MS: u32 = 250;
const RESPONSE_SIZE: usize = 11;
const READ_CHUNK_SIZE: usize = 256;
const EEPROM_LEN: usize = 256;

#[derive(Clone, Copy)]
struct HashboardConfig {
    index: usize,
    serial_path: &'static str,
    reset_gpio: u32,
    detect_gpio: u32,
}

const HASHBOARDS: [HashboardConfig; 3] = [
    HashboardConfig { index: 0, serial_path: "/dev/ttyS3", reset_gpio: 454, detect_gpio: 439 },
    HashboardConfig { index: 1, serial_path: "/dev/ttyS2", reset_gpio: 455, detect_gpio: 440 },
    HashboardConfig { index: 2, serial_path: "/dev/ttyS1", reset_gpio: 456, detect_gpio: 441 },
];

enum Command {
    Help,
    CheckAll,
    CheckOne(usize),
    Temps(usize),
    Eeprom(usize),
}

struct ResetGuard {
    gpio: SysfsGpio,
}

impl ResetGuard {
    fn new(gpio: SysfsGpio) -> Self {
        Self { gpio }
    }

    fn assert(&self) -> Result<(), std::io::Error> {
        self.gpio.set_output_low()
    }

    fn release(&self) -> Result<(), std::io::Error> {
        self.gpio.set_output_high()
    }
}

impl Drop for ResetGuard {
    fn drop(&mut self) {
        let _ = self.gpio.set_output_low();
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    match parse_args(env::args().skip(1).collect())? {
        Command::Help => print_help(),
        Command::CheckAll => {
            for board in HASHBOARDS {
                check_hashboard(board)?;
            }
        }
        Command::CheckOne(index) => check_hashboard(HASHBOARDS[index])?,
        Command::Temps(index) => read_hashboard_temps(HASHBOARDS[index])?,
        Command::Eeprom(index) => read_hashboard_eeprom(HASHBOARDS[index])?,
    }
    Ok(())
}

fn check_hashboard(board: HashboardConfig) -> Result<(), Box<dyn std::error::Error>> {
    println!();
    println!("==================================================");
    println!("Hashboard {}", board.index);
    println!("==================================================");
    println!("serial_path={}", board.serial_path);
    println!("reset_gpio={}", board.reset_gpio);
    println!("detect_gpio={}", board.detect_gpio);

    let detect = SysfsGpio::new(board.detect_gpio);
    detect.set_input_bias_disabled()?;
    let present = detect.read_value()?;
    println!("presence_detect={} ({})", present, if present == 0 { "not-present-or-low" } else { "present-or-high" });

    let reset = ResetGuard::new(SysfsGpio::new(board.reset_gpio));
    println!("toggling reset...");
    reset.assert()?;
    thread::sleep(Duration::from_millis(100));
    reset.release()?;
    thread::sleep(Duration::from_millis(100));

    let mut serial = LinuxSerialPort::open(board.serial_path, SERIAL_BAUD, SERIAL_TIMEOUT_MS)?;
    println!("sending init frame: {}", format_hex(&INIT_FRAME));
    serial.write_all(&INIT_FRAME)?;
    thread::sleep(Duration::from_millis(100));

    serial.flush_input()?;
    println!("sending ping frame: {}", format_hex(&PING_FRAME));
    serial.write_all(&PING_FRAME)?;

    let mut rx_count = 0usize;
    let mut rx_buffer = Vec::new();
    let mut model_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut unique_replies: BTreeMap<Vec<u8>, usize> = BTreeMap::new();
    loop {
        let chunk = serial.read_chunk(READ_CHUNK_SIZE)?;
        if chunk.is_empty() {
            break;
        }

        rx_buffer.extend_from_slice(&chunk);
        let frames = extract_reply_frames(&mut rx_buffer);
        for frame in frames {
            rx_count += 1;
            println!("response {:02}: {}", rx_count, format_hex(&frame));
            *model_counts.entry(asic_model_name(&frame)).or_insert(0) += 1;
            *unique_replies.entry(frame).or_insert(0) += 1;
        }
    }

    if !rx_buffer.is_empty() {
        println!("trailing_bytes={}", format_hex(&rx_buffer));
    }

    println!("response_count={rx_count}");
    println!("unique_reply_count={}", unique_replies.len());
    for (index, (frame, count)) in unique_replies.iter().enumerate() {
        println!("unique_reply {:02}: count={} data={}", index + 1, count, format_hex(frame));
    }
    println!("detected_asic_model_count={}", model_counts.len());
    for (index, (model, count)) in model_counts.iter().enumerate() {
        println!("detected_asic_model {:02}: count={} model={}", index + 1, count, model);
    }
    println!("asserting reset...");
    reset.assert()?;
    Ok(())
}

fn read_hashboard_temps(board: HashboardConfig) -> Result<(), Box<dyn std::error::Error>> {
    println!();
    println!("==================================================");
    println!("Hashboard {} temperatures", board.index);
    println!("==================================================");
    println!("i2c_device={}", TMP75_I2C_DEVICE);
    println!("detect_gpio={}", board.detect_gpio);

    let detect = SysfsGpio::new(board.detect_gpio);
    detect.set_input_bias_disabled()?;
    let present = detect.read_value()?;
    println!("presence_detect={} ({})", present, if present == 0 { "not-present-or-low" } else { "present-or-high" });
    if present == 0 {
        return Err(format!("hashboard {} is not present", board.index).into());
    }

    let sensor_addresses = tmp75_addresses(board.index)?;
    for (sensor_index, address) in sensor_addresses.iter().enumerate() {
        let raw = read_tmp75_raw(*address)?;
        let temp_c = decode_tmp75_celsius(raw);
        println!(
            "temp{}: address=0x{:02X} raw={} temp_c={:.4}",
            sensor_index,
            address,
            format_hex(&raw.to_be_bytes()),
            temp_c
        );
    }

    Ok(())
}

fn read_hashboard_eeprom(board: HashboardConfig) -> Result<(), Box<dyn std::error::Error>> {
    println!();
    println!("==================================================");
    println!("Hashboard {} EEPROM", board.index);
    println!("==================================================");
    println!("i2c_device={}", EEPROM_I2C_DEVICE);
    println!("detect_gpio={}", board.detect_gpio);

    let detect = SysfsGpio::new(board.detect_gpio);
    detect.set_input_bias_disabled()?;
    let present = detect.read_value()?;
    println!("presence_detect={} ({})", present, if present == 0 { "not-present-or-low" } else { "present-or-high" });
    if present == 0 {
        return Err(format!("hashboard {} is not present", board.index).into());
    }

    let address = eeprom_address(board.index)?;
    let eeprom = read_eeprom(address)?;
    let decoded = decode_antminer_eeprom(&eeprom)?;

    println!("eeprom_address=0x{address:02X}");
    println!("board_info_version=0x{:02X} ({:?})", eeprom[0], decoded.version);
    println!(
        "algorithm_and_key_version=0x{:02X} (algorithm={} key_index={})",
        eeprom[1],
        decoded.algorithm.name(),
        decoded.key_index
    );
    println!("raw_header={}", format_hex(&eeprom[..16]));
    println!("raw_trailer_tag=0x{:02X}", eeprom[EEPROM_LEN - 1]);
    println!();
    println!("Decoded fields:");
    println!("  board_serial={}", decoded.board_serial);
    println!("  board_name={}", decoded.board_name);
    println!("  factory_job={}", decoded.factory_job);
    println!("  chip_die={}", decoded.chip_die);
    println!("  chip_marking={}", decoded.chip_marking);
    println!("  chip_bin={}", decoded.chip_bin);
    println!("  chip_tech={}", decoded.chip_tech);
    println!("  ft_version={}", decoded.ft_version);
    println!("  pcb_version=0x{:04X}", decoded.pcb_version);
    println!("  bom_version=0x{:04X}", decoded.bom_version);
    println!("  asic_sensor_type={}", decoded.asic_sensor_type);
    println!("  asic_sensor_addr={}", format_hex(&decoded.asic_sensor_addr));
    println!("  pic_sensor_type={}", decoded.pic_sensor_type);
    println!("  pic_sensor_addr=0x{:02X}", decoded.pic_sensor_addr);
    println!("  voltage_v={:.2}", decoded.voltage_cv as f32 / 100.0);
    println!("  frequency_mhz={}", decoded.frequency_mhz);
    println!("  nonce_rate={}", decoded.nonce_rate);
    println!("  pcb_temp_in_c={}", decoded.pcb_temp_in_c);
    println!("  pcb_temp_out_c={}", decoded.pcb_temp_out_c);
    println!("  test_version={}", decoded.test_version);
    println!("  test_standard={}", decoded.test_standard);
    println!("  pt1_result={} pt1_count={}", decoded.pt1_result, decoded.pt1_count);
    println!("  pt2_result={} pt2_count={}", decoded.pt2_result, decoded.pt2_count);
    println!(
        "  pt1_crc=stored:0x{:02X} calculated:0x{:02X} match:{}",
        decoded.pt1_crc_stored,
        decoded.pt1_crc_calculated,
        decoded.pt1_crc_stored == decoded.pt1_crc_calculated
    );
    println!(
        "  pt2_crc=stored:0x{:02X} tool_calc:0x{:02X} region_calc:0x{:02X}",
        decoded.pt2_crc_stored,
        decoded.pt2_crc_calculated_tool,
        decoded.pt2_crc_calculated_region
    );

    match decoded.version {
        AntminerEepromVersion::V4 => {
            println!("  sweep=not present in v4 layout");
        }
        AntminerEepromVersion::V5 | AntminerEepromVersion::V6 => {
            println!("  sweep_hashrate={}", decoded.sweep_hashrate.unwrap_or_default());
            println!("  sweep_freq_base={}", decoded.sweep_freq_base.unwrap_or_default());
            println!("  sweep_freq_step={}", decoded.sweep_freq_step.unwrap_or_default());
            println!("  sweep_result={}", decoded.sweep_result.unwrap_or_default());
        }
    }

    println!("  sweep_non_ff={}", decoded.sweep_non_ff);
    println!("  sweep_prefix={}", format_hex(&decoded.sweep_prefix));
    println!();
    println!("Decoded bytes 0x00..0x71:");
    print_hex_dump(&decoded.decoded_bytes[..0x72]);
    println!();
    println!("Raw dump:");
    print_hex_dump(&eeprom);

    Ok(())
}

fn extract_reply_frames(buffer: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();

    loop {
        let Some(start) = find_preamble(buffer) else {
            let keep = buffer.len().min(REPLY_PREAMBLE.len().saturating_sub(1));
            if keep == 0 {
                buffer.clear();
            } else {
                let tail = buffer.split_off(buffer.len() - keep);
                buffer.clear();
                buffer.extend_from_slice(&tail);
            }
            break;
        };

        if start > 0 {
            buffer.drain(..start);
        }

        if buffer.len() < RESPONSE_SIZE {
            break;
        }

        let frame: Vec<u8> = buffer.drain(..RESPONSE_SIZE).collect();
        frames.push(frame);
    }

    frames
}

fn find_preamble(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(REPLY_PREAMBLE.len())
        .position(|window| window == REPLY_PREAMBLE)
}

fn tmp75_addresses(board_index: usize) -> Result<[u8; 2], Box<dyn std::error::Error>> {
    match board_index {
        0 => Ok([0x48, 0x4C]),
        1 => Ok([0x4D, 0x49]),
        2 => Ok([0x4E, 0x4A]),
        _ => Err(format!("invalid hashboard index: {board_index}").into()),
    }
}

fn eeprom_address(board_index: usize) -> Result<u8, Box<dyn std::error::Error>> {
    match board_index {
        0 => Ok(0x50),
        1 => Ok(0x51),
        2 => Ok(0x52),
        _ => Err(format!("invalid hashboard index: {board_index}").into()),
    }
}

fn read_eeprom(address: u8) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut device = LinuxI2cDevice::open(EEPROM_I2C_DEVICE, u16::from(address))?;

    match device.read_at(0, EEPROM_LEN) {
        Ok(data) => Ok(data),
        Err(_) => {
            let mut data = Vec::with_capacity(EEPROM_LEN);
            for offset in 0..EEPROM_LEN {
                data.push(device.read_byte_data(offset as u8)?);
            }
            Ok(data)
        }
    }
}

fn read_tmp75_raw(address: u8) -> Result<u16, Box<dyn std::error::Error>> {
    let mut device = LinuxI2cDevice::open(TMP75_I2C_DEVICE, u16::from(address))?;
    let raw = device.read_word_data(TMP75_TEMP_REG)?;
    Ok(raw.swap_bytes())
}

fn decode_tmp75_celsius(raw: u16) -> f32 {
    let value = i16::from_be_bytes(raw.to_be_bytes()) >> 4;
    value as f32 * 0.0625
}

fn print_hex_dump(data: &[u8]) {
    for (offset, chunk) in data.chunks(16).enumerate() {
        println!("{:02X}: {}", offset * 16, format_hex(chunk));
    }
}

fn parse_args(args: Vec<String>) -> Result<Command, Box<dyn std::error::Error>> {
    match args.first().map(String::as_str) {
        None | Some("help") | Some("--help") | Some("-h") => Ok(Command::Help),
        Some("check") => {
            if let Some(value) = args.get(1) {
                let index: usize = value.parse()?;
                if index >= HASHBOARDS.len() {
                    return Err(format!("invalid hashboard index: {index}").into());
                }
                Ok(Command::CheckOne(index))
            } else {
                Ok(Command::CheckAll)
            }
        }
        Some("temps") => {
            let value = args.get(1).ok_or("missing hashboard index for temps")?;
            let index: usize = value.parse()?;
            if index >= HASHBOARDS.len() {
                return Err(format!("invalid hashboard index: {index}").into());
            }
            Ok(Command::Temps(index))
        }
        Some("eeprom") => {
            let value = args.get(1).ok_or("missing hashboard index for eeprom")?;
            let index: usize = value.parse()?;
            if index >= HASHBOARDS.len() {
                return Err(format!("invalid hashboard index: {index}").into());
            }
            Ok(Command::Eeprom(index))
        }
        Some(other) => Err(format!("unknown command: {other}").into()),
    }
}

fn format_hex(data: &[u8]) -> String {
    data.iter().map(|byte| format!("{byte:02X}")).collect::<Vec<_>>().join(" ")
}

fn asic_model_name(frame: &[u8]) -> String {
    if frame.len() < 4 {
        return "unknown".to_string();
    }

    format!("BM{:02X}{:02X}", frame[2], frame[3])
}

fn print_help() {
    println!("hashboard_s19jpro");
    println!();
    println!("Sanity-check utility for the three S19j Pro hashboards connected to the Amlogic control board.");
    println!();
    println!("Known mappings:");
    println!("  hashboard0: /dev/ttyS3, reset GPIO 454 (GPIOA_17), detect GPIO 439 (GPIOA_2)");
    println!("  hashboard1: /dev/ttyS2, reset GPIO 455 (GPIOA_18), detect GPIO 440 (GPIOA_3)");
    println!("  hashboard2: /dev/ttyS1, reset GPIO 456 (GPIOA_19), detect GPIO 441 (GPIOA_4)");
    println!("  tmp75 addresses: HB0=[0x48,0x4C], HB1=[0x4D,0x49], HB2=[0x4E,0x4A]");
    println!("  eeprom addresses: HB0=0x50, HB1=0x51, HB2=0x52");
    println!();
    println!("Commands:");
    println!("  check [0|1|2]   Reset and ping one hashboard, or all three if omitted");
    println!("  temps <0|1|2>   Read both TMP75 temperature sensors on one hashboard");
    println!("  eeprom <0|1|2>  Read and summarize one hashboard EEPROM");
}
