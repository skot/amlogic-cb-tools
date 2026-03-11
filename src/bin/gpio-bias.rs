use std::env;
use std::io;
use std::os::fd::RawFd;

const PERIPHS_PULL_PHYS: usize = 0xff6344e8;
const PERIPHS_PULLEN_PHYS: usize = 0xff634520;
const AO_PULL_PHYS: usize = 0xff800024;

const PERIPHS_PULL_WORDS: usize = 5;
const AO_PULL_WORDS: usize = 1;

#[derive(Clone, Copy)]
enum Command {
    Help,
    Show { gpios: [u32; 3], count: usize },
    Set {
        mode: BiasMode,
        gpios: [u32; 3],
        count: usize,
    },
}

#[derive(Clone, Copy)]
enum BiasMode {
    Disable,
    PullUp,
    PullDown,
}

#[derive(Clone, Copy)]
enum BankKind {
    Periphs,
    Ao,
}

#[derive(Clone, Copy)]
struct BankDesc {
    linux_first: u32,
    count: u32,
    pull_reg: usize,
    pull_bit: u32,
    pullen_reg: usize,
    pullen_bit: u32,
    kind: BankKind,
}

#[derive(Clone, Copy)]
struct BiasLocation {
    kind: BankKind,
    pull_reg: usize,
    pull_bit: u32,
    pullen_reg: usize,
    pullen_bit: u32,
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
        Command::Show { gpios, count } => {
            let mut regs = BiasRegisters::open()?;
            for gpio in gpios.into_iter().take(count) {
                print_bias(&mut regs, gpio)?;
            }
        }
        Command::Set { mode, gpios, count } => {
            let mut regs = BiasRegisters::open()?;
            for gpio in gpios.into_iter().take(count) {
                regs.set_bias(gpio, mode)?;
                print_bias(&mut regs, gpio)?;
            }
        }
    }
    Ok(())
}

fn parse_args(args: Vec<String>) -> Result<Command, Box<dyn std::error::Error>> {
    match args.first().map(String::as_str) {
        None | Some("help") | Some("--help") | Some("-h") => Ok(Command::Help),
        Some("show") => {
            let (gpios, count) = parse_gpio_list(args.get(1))?;
            Ok(Command::Show { gpios, count })
        }
        Some("set") => {
            let mode = parse_bias_mode(args.get(1).ok_or("missing bias mode")?)?;
            let (gpios, count) = parse_gpio_list(args.get(2))?;
            Ok(Command::Set { mode, gpios, count })
        }
        Some(other) => Err(format!("unknown command: {other}").into()),
    }
}

fn parse_bias_mode(value: &str) -> Result<BiasMode, Box<dyn std::error::Error>> {
    match value {
        "disable" | "none" | "no-pull" => Ok(BiasMode::Disable),
        "pull-up" | "up" => Ok(BiasMode::PullUp),
        "pull-down" | "down" => Ok(BiasMode::PullDown),
        other => Err(format!("unknown bias mode: {other}").into()),
    }
}

fn parse_gpio_list(value: Option<&String>) -> Result<([u32; 3], usize), Box<dyn std::error::Error>> {
    let raw = value.ok_or("missing GPIO list")?;
    let mut gpios = [0u32; 3];
    let mut count = 0usize;

    for part in raw.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        if count >= gpios.len() {
            return Err("too many GPIOs; maximum is 3 per invocation".into());
        }
        let gpio: u32 = trimmed.parse()?;
        validate_gpio(gpio)?;
        gpios[count] = gpio;
        count += 1;
    }

    if count == 0 {
        return Err("GPIO list is empty".into());
    }

    Ok((gpios, count))
}

fn validate_gpio(gpio: u32) -> Result<(), Box<dyn std::error::Error>> {
    if !(411..=511).contains(&gpio) {
        return Err(format!("GPIO out of supported range: {gpio}").into());
    }
    Ok(())
}

fn print_help() {
    println!("gpio-bias");
    println!();
    println!("Inspect or change internal GPIO pull bias on the AXG control board.");
    println!();
    println!("Commands:");
    println!("  show <gpio0[,gpio1,...]>");
    println!("  set <disable|pull-up|pull-down> <gpio0[,gpio1,...]>");
    println!();
    println!("Examples:");
    println!("  gpio-bias show 439,440,441");
    println!("  gpio-bias set disable 439,440,441");
    println!("  gpio-bias set pull-down 439,440,441");
}

fn gpio_name(gpio: u32) -> String {
    match gpio {
        411..=421 => format!("GPIOZ_{}", gpio - 411),
        422..=436 => format!("BOOT_{}", gpio - 422),
        437..=457 => format!("GPIOA_{}", gpio - 437),
        458..=480 => format!("GPIOX_{}", gpio - 458),
        481..=496 => format!("GPIOY_{}", gpio - 481),
        497..=510 => format!("GPIOAO_{}", gpio - 497),
        511 => "GPIO_TEST_N".to_string(),
        _ => format!("GPIO_{gpio}"),
    }
}

fn bank_descs() -> &'static [BankDesc] {
    &[
        BankDesc { linux_first: 411, count: 11, pull_reg: 3, pull_bit: 0, pullen_reg: 3, pullen_bit: 0, kind: BankKind::Periphs },
        BankDesc { linux_first: 422, count: 15, pull_reg: 4, pull_bit: 0, pullen_reg: 4, pullen_bit: 0, kind: BankKind::Periphs },
        BankDesc { linux_first: 437, count: 21, pull_reg: 0, pull_bit: 0, pullen_reg: 0, pullen_bit: 0, kind: BankKind::Periphs },
        BankDesc { linux_first: 458, count: 23, pull_reg: 2, pull_bit: 0, pullen_reg: 2, pullen_bit: 0, kind: BankKind::Periphs },
        BankDesc { linux_first: 481, count: 16, pull_reg: 1, pull_bit: 0, pullen_reg: 1, pullen_bit: 0, kind: BankKind::Periphs },
        BankDesc { linux_first: 497, count: 15, pull_reg: 0, pull_bit: 0, pullen_reg: 0, pullen_bit: 16, kind: BankKind::Ao },
    ]
}

fn locate_bias(gpio: u32) -> Result<BiasLocation, Box<dyn std::error::Error>> {
    for bank in bank_descs() {
        if gpio >= bank.linux_first && gpio < bank.linux_first + bank.count {
            let bit_offset = gpio - bank.linux_first;
            return Ok(BiasLocation {
                kind: bank.kind,
                pull_reg: bank.pull_reg + ((bank.pull_bit + bit_offset) / 32) as usize,
                pull_bit: (bank.pull_bit + bit_offset) % 32,
                pullen_reg: bank.pullen_reg + ((bank.pullen_bit + bit_offset) / 32) as usize,
                pullen_bit: (bank.pullen_bit + bit_offset) % 32,
            });
        }
    }
    Err(format!("unsupported GPIO: {gpio}").into())
}

struct BiasRegisters {
    periphs_pull: MappedRegisterBlock,
    periphs_pullen: MappedRegisterBlock,
    ao_pull: MappedRegisterBlock,
}

impl BiasRegisters {
    fn open() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            periphs_pull: MappedRegisterBlock::open(PERIPHS_PULL_PHYS, PERIPHS_PULL_WORDS, true)?,
            periphs_pullen: MappedRegisterBlock::open(PERIPHS_PULLEN_PHYS, PERIPHS_PULL_WORDS, true)?,
            ao_pull: MappedRegisterBlock::open(AO_PULL_PHYS, AO_PULL_WORDS, true)?,
        })
    }

    fn get_bias(&mut self, gpio: u32) -> Result<&'static str, Box<dyn std::error::Error>> {
        let loc = locate_bias(gpio)?;
        let (pull_word, pullen_word) = match loc.kind {
            BankKind::Periphs => (
                self.periphs_pull.read_word(loc.pull_reg)?,
                self.periphs_pullen.read_word(loc.pullen_reg)?,
            ),
            BankKind::Ao => {
                let word = self.ao_pull.read_word(0)?;
                (word, word)
            }
        };

        let enabled = ((pullen_word >> loc.pullen_bit) & 1) != 0;
        let pull_up = ((pull_word >> loc.pull_bit) & 1) != 0;

        Ok(if !enabled {
            "disabled"
        } else if pull_up {
            "pull-up"
        } else {
            "pull-down"
        })
    }

    fn set_bias(&mut self, gpio: u32, mode: BiasMode) -> Result<(), Box<dyn std::error::Error>> {
        let loc = locate_bias(gpio)?;
        match loc.kind {
            BankKind::Periphs => {
                match mode {
                    BiasMode::Disable => {
                        self.periphs_pullen.update_bit(loc.pullen_reg, loc.pullen_bit, false)?;
                    }
                    BiasMode::PullUp => {
                        self.periphs_pull.update_bit(loc.pull_reg, loc.pull_bit, true)?;
                        self.periphs_pullen.update_bit(loc.pullen_reg, loc.pullen_bit, true)?;
                    }
                    BiasMode::PullDown => {
                        self.periphs_pull.update_bit(loc.pull_reg, loc.pull_bit, false)?;
                        self.periphs_pullen.update_bit(loc.pullen_reg, loc.pullen_bit, true)?;
                    }
                }
            }
            BankKind::Ao => {
                match mode {
                    BiasMode::Disable => {
                        self.ao_pull.update_bit(loc.pullen_reg, loc.pullen_bit, false)?;
                    }
                    BiasMode::PullUp => {
                        self.ao_pull.update_bit(loc.pull_reg, loc.pull_bit, true)?;
                        self.ao_pull.update_bit(loc.pullen_reg, loc.pullen_bit, true)?;
                    }
                    BiasMode::PullDown => {
                        self.ao_pull.update_bit(loc.pull_reg, loc.pull_bit, false)?;
                        self.ao_pull.update_bit(loc.pullen_reg, loc.pullen_bit, true)?;
                    }
                }
            }
        }
        Ok(())
    }
}

fn print_bias(regs: &mut BiasRegisters, gpio: u32) -> Result<(), Box<dyn std::error::Error>> {
    println!("gpio={} {} bias={}", gpio, gpio_name(gpio), regs.get_bias(gpio)?);
    Ok(())
}

struct MappedRegisterBlock {
    map_ptr: *mut libc::c_void,
    ptr: *mut u8,
    len: usize,
}

impl MappedRegisterBlock {
    fn open(phys_base: usize, words: usize, writable: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let page_size = page_size()?;
        let byte_len = words * std::mem::size_of::<u32>();
        let map_base = phys_base & !(page_size - 1);
        let page_offset = phys_base - map_base;
        let map_len = align_up(page_offset + byte_len, page_size);

        let path = "/dev/mem\0";
        let open_flags = if writable { libc::O_RDWR | libc::O_CLOEXEC } else { libc::O_RDONLY | libc::O_CLOEXEC };
        let fd = unsafe { libc::open(path.as_ptr().cast(), open_flags) };
        if fd < 0 {
            return Err(io::Error::last_os_error().into());
        }

        let prot = if writable { libc::PROT_READ | libc::PROT_WRITE } else { libc::PROT_READ };
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                map_len,
                prot,
                libc::MAP_SHARED,
                fd,
                map_base as libc::off_t,
            )
        };
        close_fd(fd);

        if ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error().into());
        }

        Ok(Self {
            map_ptr: ptr,
            ptr: unsafe { (ptr as *mut u8).add(page_offset) },
            len: map_len,
        })
    }

    fn read_word(&mut self, index: usize) -> Result<u32, Box<dyn std::error::Error>> {
        let offset = index * std::mem::size_of::<u32>();
        let value = unsafe { std::ptr::read_volatile(self.ptr.add(offset).cast::<u32>()) };
        Ok(value)
    }

    fn write_word(&mut self, index: usize, value: u32) -> Result<(), Box<dyn std::error::Error>> {
        let offset = index * std::mem::size_of::<u32>();
        unsafe { std::ptr::write_volatile(self.ptr.add(offset).cast::<u32>(), value) };
        Ok(())
    }

    fn update_bit(&mut self, index: usize, bit: u32, set: bool) -> Result<(), Box<dyn std::error::Error>> {
        let mut value = self.read_word(index)?;
        if set {
            value |= 1u32 << bit;
        } else {
            value &= !(1u32 << bit);
        }
        self.write_word(index, value)
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

fn page_size() -> Result<usize, Box<dyn std::error::Error>> {
    let value = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if value <= 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(value as usize)
}

fn close_fd(fd: RawFd) {
    let _ = unsafe { libc::close(fd) };
}