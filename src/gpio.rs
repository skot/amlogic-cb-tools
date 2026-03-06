use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct SysfsGpio {
    number: u32,
    root: PathBuf,
}

impl SysfsGpio {
    pub fn new(number: u32) -> Self {
        Self {
            number,
            root: PathBuf::from(format!("/sys/class/gpio/gpio{number}")),
        }
    }

    pub fn set_output_low(&self) -> Result<(), std::io::Error> {
        self.ensure_exported()?;
        fs::write(self.root.join("direction"), "out")?;
        fs::write(self.root.join("value"), "0")?;
        Ok(())
    }

    pub fn set_output_high(&self) -> Result<(), std::io::Error> {
        self.ensure_exported()?;
        fs::write(self.root.join("direction"), "out")?;
        fs::write(self.root.join("value"), "1")?;
        Ok(())
    }

    fn ensure_exported(&self) -> Result<(), std::io::Error> {
        if self.root.exists() {
            return Ok(());
        }

        fs::write("/sys/class/gpio/export", self.number.to_string())
    }
}
