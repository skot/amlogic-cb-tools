use std::fs;
use std::io;
use std::os::fd::RawFd;
use std::path::PathBuf;

const PERIPHS_PULLEN_PHYS: usize = 0xff634520;
const AO_PULL_PHYS: usize = 0xff800024;

const PERIPHS_PULL_WORDS: usize = 5;
const AO_PULL_WORDS: usize = 1;

#[derive(Clone, Copy)]
enum BankKind {
    Periphs,
    Ao,
}

#[derive(Clone, Copy)]
struct BankDesc {
    linux_first: u32,
    count: u32,
    pullen_reg: usize,
    pullen_bit: u32,
    kind: BankKind,
}

#[derive(Clone, Copy)]
struct BiasLocation {
    kind: BankKind,
    pullen_reg: usize,
    pullen_bit: u32,
}

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

    pub fn set_input(&self) -> Result<(), std::io::Error> {
        self.ensure_exported()?;
        fs::write(self.root.join("direction"), "in")?;
        Ok(())
    }

    pub fn set_input_bias_disabled(&self) -> Result<(), std::io::Error> {
        self.disable_bias()?;
        self.set_input()
    }

    pub fn read_value(&self) -> Result<u8, std::io::Error> {
        self.ensure_exported()?;
        let value = fs::read_to_string(self.root.join("value"))?;
        Ok(if value.trim() == "0" { 0 } else { 1 })
    }

    fn ensure_exported(&self) -> Result<(), std::io::Error> {
        if self.root.exists() {
            return Ok(());
        }

        fs::write("/sys/class/gpio/export", self.number.to_string())
    }

    fn disable_bias(&self) -> Result<(), std::io::Error> {
        let loc = locate_bias(self.number)?;
        match loc.kind {
            BankKind::Periphs => {
                let mut pullen = MappedRegisterBlock::open(PERIPHS_PULLEN_PHYS, PERIPHS_PULL_WORDS)?;
                pullen.update_bit(loc.pullen_reg, loc.pullen_bit, false)
            }
            BankKind::Ao => {
                let mut ao_pull = MappedRegisterBlock::open(AO_PULL_PHYS, AO_PULL_WORDS)?;
                ao_pull.update_bit(loc.pullen_reg, loc.pullen_bit, false)
            }
        }
    }
}

fn bank_descs() -> &'static [BankDesc] {
    &[
        BankDesc { linux_first: 411, count: 11, pullen_reg: 3, pullen_bit: 0, kind: BankKind::Periphs },
        BankDesc { linux_first: 422, count: 15, pullen_reg: 4, pullen_bit: 0, kind: BankKind::Periphs },
        BankDesc { linux_first: 437, count: 21, pullen_reg: 0, pullen_bit: 0, kind: BankKind::Periphs },
        BankDesc { linux_first: 458, count: 23, pullen_reg: 2, pullen_bit: 0, kind: BankKind::Periphs },
        BankDesc { linux_first: 481, count: 16, pullen_reg: 1, pullen_bit: 0, kind: BankKind::Periphs },
        BankDesc { linux_first: 497, count: 15, pullen_reg: 0, pullen_bit: 16, kind: BankKind::Ao },
    ]
}

fn locate_bias(gpio: u32) -> Result<BiasLocation, io::Error> {
    for bank in bank_descs() {
        if gpio >= bank.linux_first && gpio < bank.linux_first + bank.count {
            let bit_offset = gpio - bank.linux_first;
            return Ok(BiasLocation {
                kind: bank.kind,
                pullen_reg: bank.pullen_reg + ((bank.pullen_bit + bit_offset) / 32) as usize,
                pullen_bit: (bank.pullen_bit + bit_offset) % 32,
            });
        }
    }

    Err(io::Error::other(format!("unsupported GPIO for bias control: {}", gpio)))
}

struct MappedRegisterBlock {
    map_ptr: *mut libc::c_void,
    ptr: *mut u8,
    len: usize,
}

impl MappedRegisterBlock {
    fn open(phys_base: usize, words: usize) -> Result<Self, io::Error> {
        let page_size = page_size()?;
        let byte_len = words * std::mem::size_of::<u32>();
        let map_base = phys_base & !(page_size - 1);
        let page_offset = phys_base - map_base;
        let map_len = align_up(page_offset + byte_len, page_size);

        let path = "/dev/mem\0";
        let fd = unsafe { libc::open(path.as_ptr().cast(), libc::O_RDWR | libc::O_CLOEXEC) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                map_len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                map_base as libc::off_t,
            )
        };
        close_fd(fd);

        if ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }

        Ok(Self {
            map_ptr: ptr,
            ptr: unsafe { (ptr as *mut u8).add(page_offset) },
            len: map_len,
        })
    }

    fn read_word(&self, index: usize) -> u32 {
        let offset = index * std::mem::size_of::<u32>();
        unsafe { std::ptr::read_volatile(self.ptr.add(offset).cast::<u32>()) }
    }

    fn write_word(&mut self, index: usize, value: u32) {
        let offset = index * std::mem::size_of::<u32>();
        unsafe { std::ptr::write_volatile(self.ptr.add(offset).cast::<u32>(), value) };
    }

    fn update_bit(&mut self, index: usize, bit: u32, set: bool) -> Result<(), io::Error> {
        let mut value = self.read_word(index);
        if set {
            value |= 1u32 << bit;
        } else {
            value &= !(1u32 << bit);
        }
        self.write_word(index, value);
        Ok(())
    }
}

impl Drop for MappedRegisterBlock {
    fn drop(&mut self) {
        if !self.map_ptr.is_null() {
            let _ = unsafe { libc::munmap(self.map_ptr, self.len) };
        }
    }
}

fn align_up(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

fn page_size() -> Result<usize, io::Error> {
    let value = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if value <= 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(value as usize)
}

fn close_fd(fd: RawFd) {
    let _ = unsafe { libc::close(fd) };
}
