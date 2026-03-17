use embedded_graphics::mono_font::{MonoTextStyleBuilder, ascii::FONT_6X10};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use linux_embedded_hal::I2cdev;
use ssd1306::mode::BufferedGraphicsMode;
use ssd1306::prelude::*;
use ssd1306::size::DisplaySize128x32;
use ssd1306::{I2CDisplayInterface, Ssd1306};
use std::env;
use std::path::PathBuf;

const DEFAULT_I2C_DEVICE: &str = "/dev/i2c-2";
const DEFAULT_I2C_ADDRESS: u16 = 0x3C;
const DISPLAY_WIDTH: i32 = 128;
const DISPLAY_HEIGHT: i32 = 32;
const MAX_LINES: usize = 3;

struct Config {
    i2c_device: PathBuf,
    address: u16,
    lines: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            i2c_device: PathBuf::from(DEFAULT_I2C_DEVICE),
            address: DEFAULT_I2C_ADDRESS,
            lines: vec!["Hello world".to_string()],
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
    let config = parse_args(env::args().skip(1).collect())?;
    let interface = I2CDisplayInterface::new_custom_address(
        I2cdev::new(config.i2c_device.as_path())?,
        config.address as u8,
    );
    let mut display: Ssd1306<_, _, BufferedGraphicsMode<_>> =
        Ssd1306::new(interface, DisplaySize128x32, DisplayRotation::Rotate0)
            .into_buffered_graphics_mode();

    display.init().map_err(|err| format!("display init failed: {err:?}"))?;
    display
        .clear(BinaryColor::Off)
        .map_err(|err| format!("display clear failed: {err:?}"))?;
    draw_lines(&mut display, &config.lines)?;
    display.flush().map_err(|err| format!("display flush failed: {err:?}"))?;

    println!(
        "rendered {} line(s) to SSD1306 at {} address=0x{:02X}",
        config.lines.len(),
        config.i2c_device.display(),
        config.address
    );
    Ok(())
}

fn parse_args(args: Vec<String>) -> Result<Config, Box<dyn std::error::Error>> {
    let mut config = Config::default();
    let mut lines = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" | "help" => {
                print_help();
                std::process::exit(0);
            }
            "--device" => {
                index += 1;
                config.i2c_device = PathBuf::from(args.get(index).ok_or("missing value for --device")?);
            }
            "--address" => {
                index += 1;
                config.address = parse_u16(args.get(index).ok_or("missing value for --address")?)?;
            }
            other => lines.push(other.to_string()),
        }
        index += 1;
    }

    if !lines.is_empty() {
        if lines.len() > MAX_LINES {
            return Err(format!("too many lines: got {}, max is {}", lines.len(), MAX_LINES).into());
        }
        config.lines = lines;
    }

    Ok(config)
}

fn parse_u16(value: &str) -> Result<u16, Box<dyn std::error::Error>> {
    if let Some(stripped) = value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")) {
        Ok(u16::from_str_radix(stripped, 16)?)
    } else {
        Ok(value.parse()?)
    }
}

fn draw_lines(
    display: &mut Ssd1306<
        I2CInterface<I2cdev>,
        DisplaySize128x32,
        BufferedGraphicsMode<DisplaySize128x32>,
    >,
    lines: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BinaryColor::On)
        .build();

    let line_height = FONT_6X10.character_size.height as i32;
    let glyph_width = FONT_6X10.character_size.width as i32;
    let total_height = line_height * lines.len() as i32;
    let start_y = (DISPLAY_HEIGHT - total_height) / 2;

    for (index, line) in lines.iter().enumerate() {
        let rendered = line.to_ascii_uppercase();
        let text_width = glyph_width * rendered.chars().count() as i32;
        let start_x = ((DISPLAY_WIDTH - text_width).max(0)) / 2;
        let y = start_y + line_height * index as i32;

        Text::with_baseline(&rendered, Point::new(start_x, y), text_style, Baseline::Top)
            .draw(display)
            .map_err(|err| format!("display draw failed: {err:?}"))?;
    }

    Ok(())
}

fn print_help() {
    println!("oled-ssd1306");
    println!();
    println!("Initialize a 128x32 SSD1306 OLED over Linux I2C and render centered text lines.");
    println!();
    println!("Options:");
    println!("  --device <path>    Linux I2C device (default: {DEFAULT_I2C_DEVICE})");
    println!("  --address <addr>   SSD1306 I2C address (default: 0x3C)");
    println!();
    println!("Usage:");
    println!("  oled-ssd1306 [line1] [line2] [line3]");
    println!();
    println!("Examples:");
    println!("  oled-ssd1306");
    println!("  oled-ssd1306 \"Hello world\"");
    println!("  oled-ssd1306 \"Hash OK\" \"Fan 4200\"");
    println!("  oled-ssd1306 --device /dev/i2c-2 --address 0x3C \"HB0 OK\" \"PSU 12.3V\"");
}
