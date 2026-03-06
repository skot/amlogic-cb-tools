use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct SysfsTachometer {
    gpio: u32,
    gpio_root: PathBuf,
}

impl SysfsTachometer {
    pub fn new(gpio: u32) -> Self {
        Self {
            gpio,
            gpio_root: PathBuf::from(format!("/sys/class/gpio/gpio{gpio}")),
        }
    }

    pub fn gpio(&self) -> u32 {
        self.gpio
    }

    pub fn prepare(&self) -> Result<(), std::io::Error> {
        self.ensure_exported()?;
        fs::write(self.gpio_root.join("direction"), "in")?;
        fs::write(self.gpio_root.join("edge"), "falling")?;
        Ok(())
    }

    pub fn read_value(&self) -> Result<u8, std::io::Error> {
        self.ensure_exported()?;
        let text = fs::read_to_string(self.gpio_root.join("value"))?;
        Ok(if text.trim() == "0" { 0 } else { 1 })
    }

    pub fn measure_rpm(
        &self,
        duration: Duration,
        pulses_per_rev: u32,
    ) -> Result<TachReading, Box<dyn std::error::Error>> {
        self.prepare()?;
        let mut file = File::open(self.gpio_root.join("value"))?;
        let mut buf = [0u8; 8];
        let _ = file.read(&mut buf)?;

        let start = Instant::now();
        let mut pulses = 0u32;
        while start.elapsed() < duration {
            let remaining = duration.saturating_sub(start.elapsed());
            let timeout_ms = remaining.as_millis().clamp(1, i32::MAX as u128) as i32;
            let mut pfd = libc::pollfd {
                fd: file.as_raw_fd(),
                events: libc::POLLPRI,
                revents: 0,
            };
            let rc = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
            if rc < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            if rc == 0 {
                break;
            }
            if (pfd.revents & libc::POLLPRI) != 0 {
                pulses += 1;
                file.seek(SeekFrom::Start(0))?;
                let _ = file.read(&mut buf)?;
            }
        }

        let seconds = duration.as_secs_f32();
        let rpm = if seconds > 0.0 && pulses_per_rev > 0 {
            ((pulses as f32 / pulses_per_rev as f32) * (60.0 / seconds)).round() as u32
        } else {
            0
        };

        Ok(TachReading { pulses, rpm })
    }

    fn ensure_exported(&self) -> Result<(), std::io::Error> {
        if self.gpio_root.exists() {
            return Ok(());
        }
        fs::write("/sys/class/gpio/export", self.gpio.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct TachReading {
    pub pulses: u32,
    pub rpm: u32,
}