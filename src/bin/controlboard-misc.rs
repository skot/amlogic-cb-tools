use amlogic_cb_tools::gpio::SysfsGpio;
use std::env;

const GREEN_LED_GPIO: u32 = 453;
const RED_LED_GPIO: u32 = 438;
const IP_REPORT_BUTTON_GPIO: u32 = 445;

#[derive(Clone, Copy)]
struct LedConfig {
    name: &'static str,
    gpio: u32,
}

const GREEN_LED: LedConfig = LedConfig { name: "green", gpio: GREEN_LED_GPIO };
const RED_LED: LedConfig = LedConfig { name: "red", gpio: RED_LED_GPIO };
const LEDS: [LedConfig; 2] = [GREEN_LED, RED_LED];

enum Command {
    Help,
    Status,
    Set { target: LedTarget, level: OutputLevel },
    Toggle { target: LedTarget },
}

enum LedTarget {
    Green,
    Red,
    All,
}

#[derive(Clone, Copy)]
enum OutputLevel {
    Low,
    High,
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
        Command::Status => show_status()?,
        Command::Set { target, level } => set_leds(target, level)?,
        Command::Toggle { target } => toggle_leds(target)?,
    }
    Ok(())
}

fn show_status() -> Result<(), Box<dyn std::error::Error>> {
    for led in target_leds(LedTarget::All) {
        let gpio = SysfsGpio::new(led.gpio);
        let value = gpio.read_value()?;
        println!(
            "{}: gpio={} value={} state={}",
            led.name,
            led.gpio,
            value,
            if value == 0 { "low" } else { "high" }
        );
    }

    let button = SysfsGpio::new(IP_REPORT_BUTTON_GPIO);
    button.set_input()?;
    let value = button.read_value()?;
    println!(
        "ip-report-button: gpio={} value={} state={}",
        IP_REPORT_BUTTON_GPIO,
        value,
        if value == 0 { "pressed-or-low" } else { "released-or-high" }
    );
    Ok(())
}

fn set_leds(target: LedTarget, level: OutputLevel) -> Result<(), Box<dyn std::error::Error>> {
    for led in target_leds(target) {
        let gpio = SysfsGpio::new(led.gpio);
        match level {
            OutputLevel::Low => gpio.set_output_low()?,
            OutputLevel::High => gpio.set_output_high()?,
        }
        println!("{}: gpio={} set {}", led.name, led.gpio, level.as_str());
    }
    Ok(())
}

fn toggle_leds(target: LedTarget) -> Result<(), Box<dyn std::error::Error>> {
    for led in target_leds(target) {
        let gpio = SysfsGpio::new(led.gpio);
        let next = if gpio.read_value()? == 0 {
            OutputLevel::High
        } else {
            OutputLevel::Low
        };
        match next {
            OutputLevel::Low => gpio.set_output_low()?,
            OutputLevel::High => gpio.set_output_high()?,
        }
        println!("{}: gpio={} toggled to {}", led.name, led.gpio, next.as_str());
    }
    Ok(())
}

fn target_leds(target: LedTarget) -> Vec<LedConfig> {
    match target {
        LedTarget::Green => vec![GREEN_LED],
        LedTarget::Red => vec![RED_LED],
        LedTarget::All => LEDS.to_vec(),
    }
}

fn parse_args(args: Vec<String>) -> Result<Command, Box<dyn std::error::Error>> {
    match args.first().map(String::as_str) {
        None | Some("help") | Some("--help") | Some("-h") => Ok(Command::Help),
        Some("status") => Ok(Command::Status),
        Some("set") => {
            let target = parse_target(args.get(1).ok_or("missing LED target")?)?;
            let level = parse_level(args.get(2).ok_or("missing output level")?)?;
            Ok(Command::Set { target, level })
        }
        Some("toggle") => {
            let target = args
                .get(1)
                .map(|s| parse_target(s))
                .transpose()?
                .unwrap_or(LedTarget::All);
            Ok(Command::Toggle { target })
        }
        Some(other) => Err(format!("unknown command: {other}").into()),
    }
}

fn parse_target(value: &str) -> Result<LedTarget, Box<dyn std::error::Error>> {
    match value.to_ascii_lowercase().as_str() {
        "green" => Ok(LedTarget::Green),
        "red" => Ok(LedTarget::Red),
        "all" => Ok(LedTarget::All),
        _ => Err(format!("unsupported LED target: {value}").into()),
    }
}

fn parse_level(value: &str) -> Result<OutputLevel, Box<dyn std::error::Error>> {
    match value.to_ascii_lowercase().as_str() {
        "0" | "low" | "off" => Ok(OutputLevel::Low),
        "1" | "high" | "on" => Ok(OutputLevel::High),
        _ => Err(format!("unsupported output level: {value}").into()),
    }
}

impl OutputLevel {
    fn as_str(self) -> &'static str {
        match self {
            OutputLevel::Low => "low",
            OutputLevel::High => "high",
        }
    }
}

fn print_help() {
    println!("controlboard-misc");
    println!();
    println!("Amlogic control-board miscellaneous GPIO utility.");
    println!();
    println!("GPIO map:");
    println!("  - green LED: GPIO {}", GREEN_LED_GPIO);
    println!("  - red LED:   GPIO {}", RED_LED_GPIO);
    println!("  - IP report button: GPIO {}", IP_REPORT_BUTTON_GPIO);
    println!();
    println!("Commands:");
    println!("  help");
    println!("  status");
    println!("  set <green|red|all> <on|off|high|low|1|0>");
    println!("  toggle [green|red|all]");
    println!();
    println!("Examples:");
    println!("  controlboard-misc status");
    println!("  controlboard-misc set green on");
    println!("  controlboard-misc set red off");
    println!("  controlboard-misc toggle all");
}
