use amlogic_cb_tools::gpio::SysfsGpio;
use amlogic_cb_tools::serial::LinuxSerialPort;
use std::collections::BTreeMap;
use std::env;
use std::thread;
use std::time::Duration;

const INIT_FRAME: [u8; 11] = [0x55, 0xAA, 0x51, 0x09, 0x00, 0xA4, 0x90, 0x00, 0xFF, 0xFF, 0x1C];
const PING_FRAME: [u8; 7] = [0x55, 0xAA, 0x52, 0x05, 0x00, 0x00, 0x0A];
const REPLY_PREAMBLE: [u8; 2] = [0xAA, 0x55];
const SERIAL_BAUD: u32 = 115_200;
const SERIAL_TIMEOUT_MS: u32 = 250;
const RESPONSE_SIZE: usize = 11;
const READ_CHUNK_SIZE: usize = 256;

#[derive(Clone, Copy)]
struct HashboardConfig {
    index: usize,
    serial_path: &'static str,
    reset_gpio: u32,
    detect_gpio: u32,
}

const HASHBOARDS: [HashboardConfig; 3] = [
    HashboardConfig { index: 0, serial_path: "/dev/ttyS1", reset_gpio: 454, detect_gpio: 439 },
    HashboardConfig { index: 1, serial_path: "/dev/ttyS2", reset_gpio: 455, detect_gpio: 440 },
    HashboardConfig { index: 2, serial_path: "/dev/ttyS3", reset_gpio: 456, detect_gpio: 441 },
];

enum Command {
    Help,
    CheckAll,
    CheckOne(usize),
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
    detect.set_input()?;
    let present = detect.read_value()?;
    println!("presence_detect={} ({})", present, if present == 0 { "not-present-or-low" } else { "present-or-high" });

    let reset = SysfsGpio::new(board.reset_gpio);
    println!("toggling reset...");
    reset.set_output_low()?;
    thread::sleep(Duration::from_millis(100));
    reset.set_output_high()?;
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
        Some(other) => Err(format!("unknown command: {other}").into()),
    }
}

fn format_hex(data: &[u8]) -> String {
    data.iter().map(|byte| format!("{byte:02X}")).collect::<Vec<_>>().join(" ")
}

fn print_help() {
    println!("hashboard_s19jpro");
    println!();
    println!("Sanity-check utility for the three S19j Pro hashboards connected to the Amlogic control board.");
    println!();
    println!("Known mappings:");
    println!("  hashboard0: /dev/ttyS1, reset GPIO 454, detect GPIO 439");
    println!("  hashboard1: /dev/ttyS2, reset GPIO 455, detect GPIO 440");
    println!("  hashboard2: /dev/ttyS3, reset GPIO 456, detect GPIO 441");
    println!();
    println!("Commands:");
    println!("  check [0|1|2]   Reset and ping one hashboard, or all three if omitted");
}