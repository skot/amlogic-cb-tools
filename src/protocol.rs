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
/// Read the PSU's internal temperature sensor. Responds with a 2-byte
/// little-endian raw NTC/ADC code; decode with [`decode_temperature_c`].
pub const CMD_READ_TEMPERATURE: u8 = 0x09;
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

/// Breakpoint table `(breakpoint_raw, delta)` for the PSU's on-board
/// temperature sensor, lifted from the Bitmain PSU decode in `bms-miner`
/// (`psu/src/bitmain.rs`, table at 0x11722a0) as reverse engineered in
/// 256foundation/256-RedTeam@3484215.
///
/// Command 0x09 returns a raw NTC/ADC code, NOT degrees. Temperature is
/// `-30 + sum(delta for every breakpoint <= raw)`, saturating at
/// [`TEMPERATURE_SATURATED_C`] once the table is exhausted. The deltas sum to
/// 155, so the table's own end point is `-30 + 155 = 125`, consistent with the
/// saturation value.
pub const TEMPERATURE_TABLE: &[(u16, i8)] = &[
    (12, 1), (12, 1), (13, 1), (14, 1), (15, 1), (16, 1), (17, 1), (18, 1),
    (19, 1), (20, 1), (21, 1), (23, 1), (25, 1), (25, 1), (27, 1), (28, 1),
    (30, 1), (32, 1), (34, 1), (35, 1), (37, 1), (39, 1), (42, 1), (44, 1),
    (46, 1), (49, 1), (51, 1), (54, 1), (57, 1), (60, 1), (63, 1), (66, 1),
    (69, 1), (73, 1), (77, 1), (80, 1), (84, 1), (88, 1), (93, 1), (97, 1),
    (102, 1), (106, 1), (111, 1), (117, 1), (122, 1), (127, 1), (133, 1), (139, 1),
    (145, 1), (151, 1), (158, 1), (164, 1), (172, 1), (179, 1), (186, 1), (194, 1),
    (201, 1), (209, 1), (218, 1), (227, 1), (235, 1), (244, 1), (253, 1), (263, 1),
    (272, 1), (283, 1), (293, 1), (303, 1), (314, 1), (325, 1), (336, 1), (347, 1),
    (359, 1), (371, 1), (383, 1), (396, 1), (408, 1), (421, 1), (433, 1), (447, 1),
    (461, 1), (474, 1), (488, 1), (501, 1), (517, 1), (530, 1), (546, 1), (561, 1),
    (575, 1), (590, 1), (605, 1), (620, 1), (635, 1), (652, 1), (666, 1), (684, 1),
    (698, 1), (715, 1), (731, 1), (747, 1), (763, 1), (781, 1), (796, 1), (812, 1),
    (828, 1), (845, 1), (863, 1), (878, 1), (893, 1), (909, 1), (926, 1), (943, 1),
    (961, 1), (974, 1), (993, 1), (1008, 1), (1023, 1), (1039, 1), (1055, 1), (1071, 1),
    (1088, 1), (1106, 1), (1118, 1), (1137, 1), (1149, 1), (1163, 1), (1176, 1), (1196, 1),
    (1211, 1), (1225, 1), (1240, 1), (1248, 1), (1263, 1), (1279, 1), (1295, 1), (1303, 1),
    (1320, 1), (1329, 1), (1346, 1), (1355, 1), (1373, 1), (1382, 1), (1392, 1), (1411, 1),
    (1421, 1), (1431, 1), (1441, 1), (1451, 1), (1461, 1), (1472, 1), (1527, 5),
];

/// Result once `raw` reaches the end of [`TEMPERATURE_TABLE`].
pub const TEMPERATURE_SATURATED_C: i16 = 125;

/// Decode a [`CMD_READ_TEMPERATURE`] payload (2 bytes, little-endian) to
/// degrees Celsius.
pub fn decode_temperature_c(raw: u16) -> i16 {
    let mut celsius: i16 = -30;
    for &(breakpoint, delta) in TEMPERATURE_TABLE {
        if raw < breakpoint {
            return celsius;
        }
        celsius += i16::from(delta);
    }
    TEMPERATURE_SATURATED_C
}

#[cfg(test)]
mod temperature_tests {
    use super::*;

    #[test]
    fn matches_epic_reference_capture() {
        // 256-RedTeam@3484215, verified against the ePIC UI on an S19j Pro:
        // wire read `55 AA 06 09 9E 01 AE 00` -> raw 0x019E (414) -> 47 C.
        assert_eq!(decode_temperature_c(0x019E), 47);
        for raw in 412..=419 {
            assert_eq!(decode_temperature_c(raw), 47);
        }
    }

    #[test]
    fn matches_idle_capture_on_140() {
        // Live on 10.66.0.140 (APW12, type 0x76) with the output OFF and
        // ~22 C ambient: payload [9D, 00] -> raw 157 -> 20 C.
        assert_eq!(decode_temperature_c(157), 20);
    }

    #[test]
    fn saturates_past_the_table() {
        assert_eq!(decode_temperature_c(u16::MAX), TEMPERATURE_SATURATED_C);
        // The deltas must sum to exactly the saturation span.
        let total: i16 = TEMPERATURE_TABLE.iter().map(|&(_, d)| i16::from(d)).sum();
        assert_eq!(-30 + total, TEMPERATURE_SATURATED_C);
    }

    #[test]
    fn below_the_table_is_the_floor() {
        assert_eq!(decode_temperature_c(0), -30);
    }
}
