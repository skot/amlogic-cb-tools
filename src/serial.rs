use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::path::Path;

#[derive(Debug)]
pub struct LinuxSerialPort {
    file: File,
}

impl LinuxSerialPort {
    pub fn open(path: impl AsRef<Path>, baud: u32, timeout_ms: u32) -> Result<Self, Box<dyn std::error::Error>> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        configure_port(file.as_raw_fd(), baud, timeout_ms)?;
        Ok(Self { file })
    }

    pub fn write_all(&mut self, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        self.file.write_all(bytes)?;
        self.file.flush()?;
        Ok(())
    }

    pub fn flush_input(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let rc = unsafe { libc::tcflush(self.file.as_raw_fd(), libc::TCIFLUSH) };
        if rc < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }

    pub fn read_chunk(&mut self, max_len: usize) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut buf = vec![0u8; max_len];
        let count = self.file.read(&mut buf)?;
        buf.truncate(count);
        Ok(buf)
    }
}

fn configure_port(fd: i32, baud: u32, timeout_ms: u32) -> Result<(), Box<dyn std::error::Error>> {
    let mut tty = unsafe {
        let mut tty = std::mem::zeroed::<libc::termios>();
        if libc::tcgetattr(fd, &mut tty) != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        tty
    };

    unsafe {
        libc::cfmakeraw(&mut tty);
    }

    let speed = match baud {
        115200 => libc::B115200,
        57600 => libc::B57600,
        38400 => libc::B38400,
        19200 => libc::B19200,
        9600 => libc::B9600,
        _ => return Err(format!("unsupported baud rate: {baud}").into()),
    };

    unsafe {
        libc::cfsetispeed(&mut tty, speed);
        libc::cfsetospeed(&mut tty, speed);
    }

    tty.c_cflag |= libc::CLOCAL | libc::CREAD;
    tty.c_cflag &= !libc::PARENB;
    tty.c_cflag &= !libc::CSTOPB;
    tty.c_cflag &= !libc::CSIZE;
    tty.c_cflag |= libc::CS8;
    tty.c_cc[libc::VMIN] = 0;
    let deciseconds = timeout_ms.div_ceil(100).clamp(1, u8::MAX as u32) as u8;
    tty.c_cc[libc::VTIME] = deciseconds;

    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &tty) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }

    Ok(())
}