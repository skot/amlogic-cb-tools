use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SysfsPwm {
    chip: u32,
    channel: u32,
    chip_root: PathBuf,
    pwm_root: PathBuf,
}

impl SysfsPwm {
    pub fn new(chip: u32, channel: u32) -> Self {
        let chip_root = PathBuf::from(format!("/sys/class/pwm/pwmchip{chip}"));
        let pwm_root = chip_root.join(format!("pwm{channel}"));
        Self {
            chip,
            channel,
            chip_root,
            pwm_root,
        }
    }

    pub fn configure_percent(
        &self,
        period_ns: u32,
        percent: u8,
        enable: bool,
    ) -> Result<(), std::io::Error> {
        let percent = percent.min(100);
        let duty_ns = (u64::from(period_ns) * u64::from(percent) / 100) as u32;
        self.ensure_exported()?;
        self.disable()?;
        fs::write(self.pwm_root.join("period"), period_ns.to_string())?;
        fs::write(self.pwm_root.join("duty_cycle"), duty_ns.to_string())?;
        fs::write(self.pwm_root.join("polarity"), "normal")?;
        if enable {
            self.enable()?;
        }
        Ok(())
    }

    pub fn state(&self) -> Result<PwmState, std::io::Error> {
        self.ensure_exported()?;
        Ok(PwmState {
            chip: self.chip,
            channel: self.channel,
            period_ns: fs::read_to_string(self.pwm_root.join("period"))?.trim().parse().unwrap_or(0),
            duty_cycle_ns: fs::read_to_string(self.pwm_root.join("duty_cycle"))?.trim().parse().unwrap_or(0),
            enabled: fs::read_to_string(self.pwm_root.join("enable"))?.trim() == "1",
            polarity: fs::read_to_string(self.pwm_root.join("polarity"))?.trim().to_string(),
        })
    }

    fn ensure_exported(&self) -> Result<(), std::io::Error> {
        if self.pwm_root.exists() {
            return Ok(());
        }

        fs::write(self.chip_root.join("export"), self.channel.to_string())?;
        for _ in 0..20 {
            if self.pwm_root.exists() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    fn enable(&self) -> Result<(), std::io::Error> {
        fs::write(self.pwm_root.join("enable"), "1")
    }

    fn disable(&self) -> Result<(), std::io::Error> {
        if self.pwm_root.join("enable").exists() {
            let _ = fs::write(self.pwm_root.join("enable"), "0");
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PwmState {
    pub chip: u32,
    pub channel: u32,
    pub period_ns: u32,
    pub duty_cycle_ns: u32,
    pub enabled: bool,
    pub polarity: String,
}

impl PwmState {
    pub fn duty_percent(&self) -> f32 {
        if self.period_ns == 0 {
            0.0
        } else {
            (self.duty_cycle_ns as f32 * 100.0) / self.period_ns as f32
        }
    }
}