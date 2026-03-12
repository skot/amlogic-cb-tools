use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::path::Path;

const I2C_SLAVE: libc::c_ulong = 0x0703;
const I2C_SMBUS: libc::c_ulong = 0x0720;
const I2C_SMBUS_WRITE: u8 = 0;
const I2C_SMBUS_READ: u8 = 1;
const I2C_SMBUS_QUICK: u32 = 0;
const I2C_SMBUS_BYTE: u32 = 1;
const I2C_SMBUS_BYTE_DATA: u32 = 2;
const I2C_SMBUS_WORD_DATA: u32 = 3;

#[repr(C)]
union I2cSmbusData {
    byte: u8,
    word: u16,
    block: [u8; 34],
}

#[repr(C)]
struct I2cSmbusIoctlData {
    read_write: u8,
    command: u8,
    size: u32,
    data: *mut I2cSmbusData,
}

#[derive(Debug)]
pub struct LinuxI2cDevice {
    address: u16,
    file: File,
}

impl LinuxI2cDevice {
    pub fn open(path: impl AsRef<Path>, address: u16) -> Result<Self, std::io::Error> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let device = Self { address, file };
        device.set_slave_address()?;
        Ok(device)
    }

    fn set_slave_address(&self) -> Result<(), std::io::Error> {
        let rc = unsafe {
            libc::ioctl(
                self.file.as_raw_fd(),
                I2C_SLAVE as _,
                libc::c_ulong::from(self.address),
            )
        };
        if rc < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn write_byte_transaction(
        &mut self,
        register: u8,
        byte: u8,
    ) -> Result<(), std::io::Error> {
        let mut data = I2cSmbusData { byte };
        let mut args = I2cSmbusIoctlData {
            read_write: I2C_SMBUS_WRITE,
            command: register,
            size: I2C_SMBUS_BYTE_DATA,
            data: &mut data,
        };

        let rc = unsafe { libc::ioctl(self.file.as_raw_fd(), I2C_SMBUS as _, &mut args) };
        if rc < 0 {
            return Err(std::io::Error::last_os_error());
        }

        Ok(())
    }

    pub fn quick_write(&mut self) -> Result<(), std::io::Error> {
        let mut args = I2cSmbusIoctlData {
            read_write: I2C_SMBUS_WRITE,
            command: 0,
            size: I2C_SMBUS_QUICK,
            data: std::ptr::null_mut(),
        };

        let rc = unsafe { libc::ioctl(self.file.as_raw_fd(), I2C_SMBUS as _, &mut args) };
        if rc < 0 {
            return Err(std::io::Error::last_os_error());
        }

        Ok(())
    }

    pub fn read_byte_transaction(&mut self) -> Result<u8, std::io::Error> {
        let mut data = I2cSmbusData { byte: 0 };
        let mut args = I2cSmbusIoctlData {
            read_write: I2C_SMBUS_READ,
            command: 0,
            size: I2C_SMBUS_BYTE,
            data: &mut data,
        };

        let rc = unsafe { libc::ioctl(self.file.as_raw_fd(), I2C_SMBUS as _, &mut args) };
        if rc < 0 {
            return Err(std::io::Error::last_os_error());
        }

        let byte = unsafe { data.byte };
        Ok(byte)
    }

    pub fn read_word_data(&mut self, register: u8) -> Result<u16, std::io::Error> {
        let mut data = I2cSmbusData { word: 0 };
        let mut args = I2cSmbusIoctlData {
            read_write: I2C_SMBUS_READ,
            command: register,
            size: I2C_SMBUS_WORD_DATA,
            data: &mut data,
        };

        let rc = unsafe { libc::ioctl(self.file.as_raw_fd(), I2C_SMBUS as _, &mut args) };
        if rc < 0 {
            return Err(std::io::Error::last_os_error());
        }

        let word = unsafe { data.word };
        Ok(word)
    }

    pub fn read_byte_data(&mut self, register: u8) -> Result<u8, std::io::Error> {
        let mut data = I2cSmbusData { byte: 0 };
        let mut args = I2cSmbusIoctlData {
            read_write: I2C_SMBUS_READ,
            command: register,
            size: I2C_SMBUS_BYTE_DATA,
            data: &mut data,
        };

        let rc = unsafe { libc::ioctl(self.file.as_raw_fd(), I2C_SMBUS as _, &mut args) };
        if rc < 0 {
            return Err(std::io::Error::last_os_error());
        }

        let byte = unsafe { data.byte };
        Ok(byte)
    }

    pub fn read_at(&mut self, register: u8, len: usize) -> Result<Vec<u8>, std::io::Error> {
        self.file.seek(SeekFrom::Start(u64::from(register)))?;

        let mut buf = vec![0u8; len];
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }

    pub fn write(&mut self, data: &[u8]) -> Result<(), std::io::Error> {
        self.file.write_all(data)
    }
}
