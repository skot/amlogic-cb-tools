use std::fmt;

pub const PREAMBLE_LSB: u8 = 0x55;
pub const PREAMBLE_MSB: u8 = 0xAA;
pub const NAK_BYTE: u8 = 0xF5;
pub const DEFAULT_PSU_ADDRESS: u16 = 0x10;
pub const DEFAULT_PSU_WRITE_REGISTER: u8 = 0x11;

pub const CMD_GET_FW_VERSION: u8 = 0x01;
pub const CMD_GET_HW_VERSION: u8 = 0x02;
pub const CMD_GET_VOLTAGE: u8 = 0x03;
pub const CMD_MEASURE_VOLTAGE: u8 = 0x04;
pub const CMD_READ_STATE: u8 = 0x05;
pub const CMD_READ_CAL: u8 = 0x06;
pub const CMD_WATCHDOG: u8 = 0x81;
pub const CMD_SET_VOLTAGE: u8 = 0x83;
pub const CMD_WRITE_CAL: u8 = 0x86;

pub const DAC_REF_VOLTS: f32 = 15.1084;
pub const DAC_OFFSET_VOLTS_PER_COUNT: f32 = -0.013046;

#[derive(Debug)]
pub enum ProtocolError {
    EmptyResponse,
    Nak,
    InvalidPreamble(Vec<u8>),
    InvalidLength { declared: usize, actual: usize },
    InvalidChecksum { expected: u8, actual: u8 },
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyResponse => write!(f, "empty response"),
            Self::Nak => write!(f, "PSU returned NAK (0xF5)"),
            Self::InvalidPreamble(bytes) => write!(f, "invalid preamble: {:02X?}", bytes),
            Self::InvalidLength { declared, actual } => {
                write!(f, "invalid frame length: declared {}, actual {}", declared, actual)
            }
            Self::InvalidChecksum { expected, actual } => write!(
                f,
                "invalid checksum: expected 0x{expected:02X}, got 0x{actual:02X}"
            ),
        }
    }
}

impl std::error::Error for ProtocolError {}

#[derive(Debug, Clone)]
pub struct Frame {
    pub command: u8,
    pub payload: Vec<u8>,
    pub raw: Vec<u8>,
}

pub fn checksum(length: u8, command: u8, payload: &[u8]) -> u16 {
    payload
        .iter()
        .fold(u16::from(length) + u16::from(command), |sum, byte| {
            sum + u16::from(*byte)
        })
}

pub fn build_frame(command: u8, payload: &[u8]) -> Vec<u8> {
    let length = (payload.len() + 4) as u8;
    let checksum = checksum(length, command, payload);

    let mut frame = Vec::with_capacity(payload.len() + 6);
    frame.push(PREAMBLE_LSB);
    frame.push(PREAMBLE_MSB);
    frame.push(length);
    frame.push(command);
    frame.extend_from_slice(payload);
    frame.push((checksum & 0x00FF) as u8);
    frame.push((checksum >> 8) as u8);
    frame
}

pub fn parse_frame(raw: &[u8]) -> Result<Frame, ProtocolError> {
    if raw.is_empty() {
        return Err(ProtocolError::EmptyResponse);
    }
    if raw == [NAK_BYTE] {
        return Err(ProtocolError::Nak);
    }
    if raw.len() < 6 {
        return Err(ProtocolError::InvalidLength {
            declared: raw.get(2).copied().unwrap_or_default() as usize,
            actual: raw.len(),
        });
    }
    if raw[0] != PREAMBLE_LSB || raw[1] != PREAMBLE_MSB {
        return Err(ProtocolError::InvalidPreamble(raw[..raw.len().min(2)].to_vec()));
    }

    let declared_len = raw[2] as usize;
    let actual_len_from_length = raw.len().saturating_sub(2);
    if declared_len != actual_len_from_length {
        return Err(ProtocolError::InvalidLength {
            declared: declared_len,
            actual: actual_len_from_length,
        });
    }

    let command = raw[3];
    let checksum_index = raw.len() - 2;
    let payload = &raw[4..checksum_index];
    let actual_checksum = raw[checksum_index];
    let expected_checksum = checksum(raw[2], command, payload) as u8;
    if actual_checksum != expected_checksum {
        return Err(ProtocolError::InvalidChecksum {
            expected: expected_checksum,
            actual: actual_checksum,
        });
    }

    Ok(Frame {
        command,
        payload: payload.to_vec(),
        raw: raw.to_vec(),
    })
}

pub fn decode_dac_to_voltage(dac: u8) -> f32 {
    DAC_REF_VOLTS + DAC_OFFSET_VOLTS_PER_COUNT * f32::from(dac)
}

pub fn encode_voltage_to_dac(voltage: f32) -> u8 {
    let code = ((voltage - DAC_REF_VOLTS) / DAC_OFFSET_VOLTS_PER_COUNT).round();
    code.clamp(0.0, 255.0) as u8
}

pub fn decode_measured_voltage(adc_lo: u8, adc_hi: u8) -> f32 {
    let raw = u16::from(adc_lo) | (u16::from(adc_hi) << 8);
    (raw as f32 + 0.8615) / 63.017
}
