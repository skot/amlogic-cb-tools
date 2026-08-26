//! Antminer hashboard PIC microcontroller driver.
//!
//! Implements the I2C protocol used to communicate with the on-hashboard
//! PIC microcontroller (PIC16F1704) on PIC-variant Antminer hashboards
//! (BHB42601 / S19j Pro and family). The PIC gates the per-domain DC-DC
//! regulators, holds calibration, and proxies temperature sensors.
//!
//! Frame format (host -> PIC):
//!   [0x55, 0xAA, length, opcode, payload..., crc]
//!   - length = number of bytes from `length` onwards (inclusive of crc)
//!   - crc    = (length + opcode + sum(payload)) & 0xFF
//!
//! Critical timing: the PIC streams responses one byte at a time with
//! ~30 ms gaps between bytes. A single block read of N bytes returns
//! garbage; mirror the byte-by-byte read pattern.
//!
//! The PIC requires PSU output to be ON to respond — its onboard LDO is
//! fed from the 12 V rail.
//!
//! Bring-up sequence captured from LuxOS via i2c ftrace:
//!   1. reset_pic
//!   2. start_app    (bootloader -> application jump)
//!   3. get_sw_ver
//!   4. disable_dc_dc        (ensure clean state)
//!   5. (optional) write_reg(0x48, 0x0000)
//!   6. heart_beat
//!   7. read 4x temp regs at 0x48..0x4b   (initial baseline)
//!   8. enable_dc_dc          <-- chips power up here
//!   9. periodic heart_beat   (~every 1.5 s)

use std::fmt;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use crate::linux_i2c::LinuxI2cDevice;

/// Default I2C bus path on the Amlogic A113D control board.
pub const DEFAULT_I2C_DEVICE: &str = "/dev/i2c-0";

/// PIC I2C address mapping derived from hashboard slot index.
/// HB0 -> 0x20, HB1 -> 0x21, HB2 -> 0x22.
pub fn pic_address_for_slot(slot: u8) -> u16 {
    0x20 + u16::from(slot)
}

const PREAMBLE_0: u8 = 0x55;
const PREAMBLE_1: u8 = 0xAA;

const OP_START_APP: u8 = 0x06;
const OP_RESET: u8 = 0x07;
const OP_SET_DC_DC: u8 = 0x15;
const OP_HEARTBEAT: u8 = 0x16;
const OP_VERSION: u8 = 0x17;
const OP_READ_REG: u8 = 0x3c;

const READ_BYTE_GAP: Duration = Duration::from_millis(30);
const POST_WRITE_DELAY: Duration = Duration::from_millis(300);
const POST_RESET_DELAY: Duration = Duration::from_millis(500);

#[derive(Debug)]
pub enum PicError {
    Io {
        addr: u16,
        source: std::io::Error,
    },
    UnexpectedResponse {
        addr: u16,
        opcode: u8,
        raw: Vec<u8>,
    },
}

impl fmt::Display for PicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PicError::Io { addr, source } => {
                write!(f, "I/O error talking to PIC at 0x{addr:02x}: {source}")
            }
            PicError::UnexpectedResponse { addr, opcode, raw } => write!(
                f,
                "unexpected PIC response at 0x{addr:02x} for opcode 0x{opcode:02x}: {raw:02x?}"
            ),
        }
    }
}

impl std::error::Error for PicError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PicError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub struct PicChain {
    dev: LinuxI2cDevice,
    addr: u16,
}

impl PicChain {
    pub fn open(device: impl AsRef<Path>, addr: u16) -> Result<Self, PicError> {
        let dev = LinuxI2cDevice::open(device.as_ref(), addr).map_err(|e| PicError::Io {
            addr,
            source: e,
        })?;
        Ok(Self { dev, addr })
    }

    pub fn address(&self) -> u16 {
        self.addr
    }

    /// Build a frame `[0x55, 0xAA, length, opcode, payload..., crc]`.
    fn build_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
        let length = (payload.len() + 3) as u8; // +1 length itself, +1 opcode, +1 crc
        let mut frame = Vec::with_capacity(usize::from(length) + 2);
        frame.push(PREAMBLE_0);
        frame.push(PREAMBLE_1);
        frame.push(length);
        frame.push(opcode);
        frame.extend_from_slice(payload);
        let sum: u32 = u32::from(length)
            + u32::from(opcode)
            + payload.iter().copied().map(u32::from).sum::<u32>();
        frame.push((sum & 0xFF) as u8);
        frame
    }

    /// Single PIC exchange: write request, read N bytes one at a time
    /// with READ_BYTE_GAP between each (mirrors LuxOS behavior).
    fn exchange(
        &mut self,
        opcode: u8,
        payload: &[u8],
        response_len: usize,
        post_write_delay: Duration,
    ) -> Result<Vec<u8>, PicError> {
        let frame = Self::build_frame(opcode, payload);
        self.dev.write(&frame).map_err(|e| PicError::Io {
            addr: self.addr,
            source: e,
        })?;
        sleep(post_write_delay);

        let mut buf = Vec::with_capacity(response_len);
        for _ in 0..response_len {
            let mut one = [0u8; 1];
            self.dev.raw_read(&mut one).map_err(|e| PicError::Io {
                addr: self.addr,
                source: e,
            })?;
            buf.push(one[0]);
            sleep(READ_BYTE_GAP);
        }
        Ok(buf)
    }

    /// Reset PIC: returns to bootloader. Must be followed by start_app to
    /// regain application functionality.
    pub fn reset(&mut self) -> Result<(), PicError> {
        let resp = self.exchange(OP_RESET, &[0x00], 2, POST_WRITE_DELAY)?;
        if resp == [0x07, 0x01] {
            sleep(POST_RESET_DELAY);
            Ok(())
        } else {
            Err(PicError::UnexpectedResponse {
                addr: self.addr,
                opcode: OP_RESET,
                raw: resp,
            })
        }
    }

    /// Bootloader -> application jump.
    pub fn start_app(&mut self) -> Result<(), PicError> {
        let resp = self.exchange(OP_START_APP, &[0x00], 2, POST_WRITE_DELAY)?;
        if resp == [0x06, 0x01] {
            sleep(Duration::from_millis(300));
            Ok(())
        } else {
            Err(PicError::UnexpectedResponse {
                addr: self.addr,
                opcode: OP_START_APP,
                raw: resp,
            })
        }
    }

    /// Read PIC firmware version. Response: `[0x05, 0x17, version, ?, crc]`.
    pub fn get_sw_version(&mut self) -> Result<u8, PicError> {
        let resp = self.exchange(OP_VERSION, &[0x00], 5, POST_WRITE_DELAY)?;
        if resp.len() == 5 && resp[0] == 0x05 && resp[1] == 0x17 {
            Ok(resp[2])
        } else {
            Err(PicError::UnexpectedResponse {
                addr: self.addr,
                opcode: OP_VERSION,
                raw: resp,
            })
        }
    }

    /// Send a heartbeat ping.
    pub fn heartbeat(&mut self) -> Result<(), PicError> {
        let resp = self.exchange(OP_HEARTBEAT, &[0x00], 6, Duration::from_millis(10))?;
        // Response: [0x06, 0x16, 0x01, 0x00, 0x00, crc]
        if resp.len() == 6 && resp[1] == 0x16 && resp[2] == 0x01 {
            Ok(())
        } else {
            Err(PicError::UnexpectedResponse {
                addr: self.addr,
                opcode: OP_HEARTBEAT,
                raw: resp,
            })
        }
    }

    /// Enable per-domain DC-DC regulators. Chips will power up afterwards
    /// and start drawing current.
    pub fn enable_dc_dc(&mut self) -> Result<(), PicError> {
        self.set_dc_dc(true)
    }

    /// Disable per-domain DC-DC regulators. Chips will lose power.
    pub fn disable_dc_dc(&mut self) -> Result<(), PicError> {
        self.set_dc_dc(false)
    }

    fn set_dc_dc(&mut self, enable: bool) -> Result<(), PicError> {
        let payload = [if enable { 0x01 } else { 0x00 }, 0x00];
        let resp = self.exchange(OP_SET_DC_DC, &payload, 2, POST_WRITE_DELAY)?;
        if resp == [0x15, 0x01] {
            Ok(())
        } else {
            Err(PicError::UnexpectedResponse {
                addr: self.addr,
                opcode: OP_SET_DC_DC,
                raw: resp,
            })
        }
    }

    /// Read a 16-bit value from PIC internal register `reg`. Used for
    /// temperature sensors (reg 0x48..0x4b on BHB42601).
    pub fn read_reg(&mut self, reg: u8) -> Result<u16, PicError> {
        let payload = [reg, 0x02, 0x00];
        // Response: [0x07, 0x3c, 0x01, data_lo, data_hi, 0x00, crc] = 7 bytes
        let resp = self.exchange(OP_READ_REG, &payload, 7, POST_WRITE_DELAY)?;
        if resp.len() == 7 && resp[1] == OP_READ_REG && resp[2] == 0x01 {
            Ok(u16::from(resp[3]) | (u16::from(resp[4]) << 8))
        } else {
            Err(PicError::UnexpectedResponse {
                addr: self.addr,
                opcode: OP_READ_REG,
                raw: resp,
            })
        }
    }

    /// Run the standard handshake: reset -> start_app -> get_sw_ver ->
    /// disable_dc_dc. Leaves the PIC in application mode with chips still
    /// unpowered; caller can then `enable_dc_dc()` when ready.
    pub fn handshake(&mut self) -> Result<u8, PicError> {
        self.reset()?;
        self.start_app()?;
        let version = self.get_sw_version()?;
        self.disable_dc_dc()?;
        Ok(version)
    }

    /// Read the four PIC-mediated temperature sensors at registers
    /// 0x48..0x4b. Each is a 16-bit value; byte 0 (low) is the integer
    /// degrees Celsius (matches what LuxOS reports in its API).
    /// Returns the four temperatures in register order.
    pub fn read_temperatures_celsius(&mut self) -> Result<[f32; 4], PicError> {
        let mut out = [0f32; 4];
        for (slot, reg) in (0x48u8..=0x4bu8).enumerate() {
            let raw = self.read_reg(reg)?;
            // Byte 0 (low) = integer °C. The high byte contains
            // sensor-calibration / fractional bits that vary per sensor;
            // we don't decode those for now.
            out[slot] = (raw & 0xFF) as f32;
        }
        Ok(out)
    }
}
