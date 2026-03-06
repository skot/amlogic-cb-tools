use amlogic_cb_tools::pwm::SysfsPwm;
use amlogic_cb_tools::tach::SysfsTachometer;
use std::env;
use std::time::Duration;

const FAN_PWM_CHIP: u32 = 0;
const FAN_PWM_PERIOD_NS: u32 = 10_000;
const FAN_TACH_PULSES_PER_REV: u32 = 2;
const DEFAULT_MEASURE_MILLIS: u64 = 1_000;
const PWM_CHANNELS: [u32; 2] = [0, 1];
const FAN_GPIO_MAP: [u32; 4] = [447, 448, 449, 450];

enum Command {
    Help,
    GetPwm,
    SetPercent(u8),
    SetPwm { target: PwmTarget, percent: u8 },
    ReadRpm { target: FanTarget, duration_ms: u64 },
}

enum PwmTarget {
    All,
    Channel(u32),
}

enum FanTarget {
    All,
    Fan(usize),
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
        Command::GetPwm => show_pwm()?,
        Command::SetPercent(percent) => set_pwm(PwmTarget::All, percent)?,
        Command::SetPwm { target, percent } => set_pwm(target, percent)?,
        Command::ReadRpm { target, duration_ms } => read_rpm(target, duration_ms)?,
    }
    Ok(())
}

fn show_pwm() -> Result<(), Box<dyn std::error::Error>> {
    for channel in PWM_CHANNELS {
        let pwm = SysfsPwm::new(FAN_PWM_CHIP, channel);
        let state = pwm.state()?;
        println!(
            "pwm{}: enabled={} period_ns={} duty_cycle_ns={} duty_percent={:.1} polarity={}",
            state.channel,
            state.enabled,
            state.period_ns,
            state.duty_cycle_ns,
            state.duty_percent(),
            state.polarity
        );
    }
    Ok(())
}

fn set_pwm(target: PwmTarget, percent: u8) -> Result<(), Box<dyn std::error::Error>> {
    let channels: Vec<u32> = match target {
        PwmTarget::All => PWM_CHANNELS.to_vec(),
        PwmTarget::Channel(channel) => vec![channel],
    };

    for channel in channels {
        let pwm = SysfsPwm::new(FAN_PWM_CHIP, channel);
        pwm.configure_percent(FAN_PWM_PERIOD_NS, percent, true)?;
        let state = pwm.state()?;
        println!(
            "pwm{} set to {:.1}% (enabled={} period_ns={} duty_cycle_ns={})",
            channel,
            state.duty_percent(),
            state.enabled,
            state.period_ns,
            state.duty_cycle_ns
        );
    }
    Ok(())
}

fn read_rpm(target: FanTarget, duration_ms: u64) -> Result<(), Box<dyn std::error::Error>> {
    let fans: Vec<(usize, u32)> = match target {
        FanTarget::All => FAN_GPIO_MAP.iter().copied().enumerate().collect(),
        FanTarget::Fan(index) => vec![(index, FAN_GPIO_MAP[index])],
    };

    for (index, gpio) in fans {
        let tach = SysfsTachometer::new(gpio);
        let value = tach.read_value()?;
        let reading = tach.measure_rpm(Duration::from_millis(duration_ms), FAN_TACH_PULSES_PER_REV)?;
        println!(
            "fan{}: gpio={} value={} pulses={} rpm={} window_ms={} ppr={}",
            index,
            tach.gpio(),
            value,
            reading.pulses,
            reading.rpm,
            duration_ms,
            FAN_TACH_PULSES_PER_REV
        );
    }
    Ok(())
}

fn parse_args(args: Vec<String>) -> Result<Command, Box<dyn std::error::Error>> {
    match args.first().map(String::as_str) {
        None | Some("help") | Some("--help") | Some("-h") => Ok(Command::Help),
        Some("get-pwm") => Ok(Command::GetPwm),
        Some("set-percent") => {
            let percent = parse_percent(args.get(1).ok_or("missing percent")?)?;
            Ok(Command::SetPercent(percent))
        }
        Some("set-pwm") => {
            let target = parse_pwm_target(args.get(1).ok_or("missing pwm target")?)?;
            let percent = parse_percent(args.get(2).ok_or("missing percent")?)?;
            Ok(Command::SetPwm { target, percent })
        }
        Some("read-rpm") => {
            let target = args
                .get(1)
                .map(|s| parse_fan_target(s))
                .transpose()?
                .unwrap_or(FanTarget::All);
            let duration_ms = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(DEFAULT_MEASURE_MILLIS);
            Ok(Command::ReadRpm { target, duration_ms })
        }
        Some(other) => Err(format!("unknown command: {other}").into()),
    }
}

fn parse_pwm_target(value: &str) -> Result<PwmTarget, Box<dyn std::error::Error>> {
    if value.eq_ignore_ascii_case("all") {
        return Ok(PwmTarget::All);
    }
    let channel: u32 = value.parse()?;
    if !PWM_CHANNELS.contains(&channel) {
        return Err(format!("unsupported pwm channel: {channel}").into());
    }
    Ok(PwmTarget::Channel(channel))
}

fn parse_fan_target(value: &str) -> Result<FanTarget, Box<dyn std::error::Error>> {
    if value.eq_ignore_ascii_case("all") {
        return Ok(FanTarget::All);
    }
    let index: usize = value.parse()?;
    if index >= FAN_GPIO_MAP.len() {
        return Err(format!("unsupported fan index: {index}").into());
    }
    Ok(FanTarget::Fan(index))
}

fn parse_percent(value: &str) -> Result<u8, Box<dyn std::error::Error>> {
    let percent: u8 = value.parse()?;
    if percent > 100 {
        return Err("percent must be in 0..=100".into());
    }
    Ok(percent)
}

fn print_help() {
    println!("fan-tool");
    println!();
    println!("Amlogic control-board fan PWM and tachometer utility.");
    println!();
    println!("Assumptions:");
    println!("  - Tach inputs are on GPIO 447..450");
    println!("  - Fan PWM outputs are pwmchip0/pwm0 and pwmchip0/pwm1");
    println!("  - PWM period defaults to 100 kHz (10000 ns)");
    println!("  - Fans output 2 tach pulses per revolution");
    println!();
    println!("Commands:");
    println!("  help");
    println!("  get-pwm");
    println!("  set-percent <percent>");
    println!("  set-pwm <all|0|1> <percent>");
    println!("  read-rpm [all|0|1|2|3] [window_ms]");
    println!();
    println!("Examples:");
    println!("  fan-tool get-pwm");
    println!("  fan-tool set-percent 15");
    println!("  fan-tool set-pwm all 100");
    println!("  fan-tool set-pwm 0 65");
    println!("  fan-tool read-rpm all 1000");
}