const EEPROM_SIZE: usize = 256;
const REGION1_START: usize = 2;
const REGION1_SIZE: usize = 96;
const REGION1_CRC_POS: usize = 97;
const REGION1_CRC_BITS: usize = 97 * 8;
const REGION2_START: usize = 98;
const REGION2_SIZE: usize = 16;
const REGION2_CRC_POS: usize = 113;
const REGION3_START: usize = 114;
const REGION3_SIZE: usize = 136;
const DELTA: u32 = 0x9E37_79B9;

const KEY_LARGE: [[u8; 16]; 4] = [
    *b"ilijnaiaayuxnixo",
    [0x74, 0x51, 0xED, 0x7C, 0x7B, 0x5C, 0xD8, 0x72, 0x17, 0x4F, 0xE0, 0x79, 0x0A, 0x15, 0xE4, 0xF5],
    *b"uohzoahzuhidkgna",
    *b"uileynimdpfnangr",
];

const KEY_SMALL: [u32; 4] = [0xBABE_FACE, 0xFEED_CAFE, 0xDEAD_BEEF, 0xABCD_55AA];

const CRC5_LOOKUP: [u8; 256] = [
    0x00, 0x28, 0x50, 0x78, 0xA0, 0x88, 0xF0, 0xD8, 0x68, 0x40, 0x38, 0x10, 0xC8, 0xE0, 0x98, 0xB0,
    0xD0, 0xF8, 0x80, 0xA8, 0x70, 0x58, 0x20, 0x08, 0xB8, 0x90, 0xE8, 0xC0, 0x18, 0x30, 0x48, 0x60,
    0x88, 0xA0, 0xD8, 0xF0, 0x28, 0x00, 0x78, 0x50, 0xE0, 0xC8, 0xB0, 0x98, 0x40, 0x68, 0x10, 0x38,
    0x58, 0x70, 0x08, 0x20, 0xF8, 0xD0, 0xA8, 0x80, 0x30, 0x18, 0x60, 0x48, 0x90, 0xB8, 0xC0, 0xE8,
    0x38, 0x10, 0x68, 0x40, 0x98, 0xB0, 0xC8, 0xE0, 0x50, 0x78, 0x00, 0x28, 0xF0, 0xD8, 0xA0, 0x88,
    0xE8, 0xC0, 0xB8, 0x90, 0x48, 0x60, 0x18, 0x30, 0x80, 0xA8, 0xD0, 0xF8, 0x20, 0x08, 0x70, 0x58,
    0xB0, 0x98, 0xE0, 0xC8, 0x10, 0x38, 0x40, 0x68, 0xD8, 0xF0, 0x88, 0xA0, 0x78, 0x50, 0x28, 0x00,
    0x60, 0x48, 0x30, 0x18, 0xC0, 0xE8, 0x90, 0xB8, 0x08, 0x20, 0x58, 0x70, 0xA8, 0x80, 0xF8, 0xD0,
    0x70, 0x58, 0x20, 0x08, 0xD0, 0xF8, 0x80, 0xA8, 0x18, 0x30, 0x48, 0x60, 0xB8, 0x90, 0xE8, 0xC0,
    0xA0, 0x88, 0xF0, 0xD8, 0x00, 0x28, 0x50, 0x78, 0xC8, 0xE0, 0x98, 0xB0, 0x68, 0x40, 0x38, 0x10,
    0xF8, 0xD0, 0xA8, 0x80, 0x58, 0x70, 0x08, 0x20, 0x90, 0xB8, 0xC0, 0xE8, 0x30, 0x18, 0x60, 0x48,
    0x28, 0x00, 0x78, 0x50, 0x88, 0xA0, 0xD8, 0xF0, 0x40, 0x68, 0x10, 0x38, 0xE0, 0xC8, 0xB0, 0x98,
    0x48, 0x60, 0x18, 0x30, 0xE8, 0xC0, 0xB8, 0x90, 0x20, 0x08, 0x70, 0x58, 0x80, 0xA8, 0xD0, 0xF8,
    0x98, 0xB0, 0xC8, 0xE0, 0x38, 0x10, 0x68, 0x40, 0xF0, 0xD8, 0xA0, 0x88, 0x50, 0x78, 0x00, 0x28,
    0xC0, 0xE8, 0x90, 0xB8, 0x60, 0x48, 0x30, 0x18, 0xA8, 0x80, 0xF8, 0xD0, 0x08, 0x20, 0x58, 0x70,
    0x10, 0x38, 0x40, 0x68, 0xB0, 0x98, 0xE0, 0xC8, 0x78, 0x50, 0x28, 0x00, 0xD8, 0xF0, 0x88, 0xA0,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntminerEepromVersion {
    V4,
    V5,
    V6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntminerEepromAlgorithm {
    Xxtea,
    Xor,
    Unknown(u8),
}

impl AntminerEepromAlgorithm {
    pub fn name(self) -> &'static str {
        match self {
            Self::Xxtea => "XXTEA",
            Self::Xor => "XOR",
            Self::Unknown(_) => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DecodedAntminerEeprom {
    pub version: AntminerEepromVersion,
    pub algorithm: AntminerEepromAlgorithm,
    pub key_index: u8,
    pub decoded_bytes: Vec<u8>,
    pub board_serial: String,
    pub chip_die: String,
    pub chip_marking: String,
    pub chip_bin: u8,
    pub ft_version: String,
    pub pcb_version: u16,
    pub bom_version: u16,
    pub asic_sensor_type: u8,
    pub asic_sensor_addr: [u8; 4],
    pub pic_sensor_type: u8,
    pub pic_sensor_addr: u8,
    pub chip_tech: String,
    pub board_name: String,
    pub factory_job: String,
    pub pt1_result: u8,
    pub pt1_count: u8,
    pub pt1_crc_stored: u8,
    pub pt1_crc_calculated: u8,
    pub voltage_cv: u16,
    pub frequency_mhz: u16,
    pub nonce_rate: u16,
    pub pcb_temp_in_c: i8,
    pub pcb_temp_out_c: i8,
    pub test_version: u8,
    pub test_standard: u8,
    pub pt2_result: u8,
    pub pt2_count: u8,
    pub pt2_crc_stored: u8,
    pub pt2_crc_calculated_tool: u8,
    pub pt2_crc_calculated_region: u8,
    pub sweep_hashrate: Option<u16>,
    pub sweep_freq_base: Option<u16>,
    pub sweep_freq_step: Option<u8>,
    pub sweep_result: Option<u8>,
    pub sweep_non_ff: usize,
    pub sweep_prefix: Vec<u8>,
}

pub fn decode_antminer_eeprom(data: &[u8]) -> Result<DecodedAntminerEeprom, String> {
    if data.len() != EEPROM_SIZE {
        return Err(format!("expected {EEPROM_SIZE} bytes, got {}", data.len()));
    }

    let version = match data[0] {
        4 => AntminerEepromVersion::V4,
        5 => AntminerEepromVersion::V5,
        6 => AntminerEepromVersion::V6,
        other => return Err(format!("unsupported Antminer EEPROM version: 0x{other:02X}")),
    };

    let algorithm_raw = data[1] >> 4;
    let key_index = data[1] & 0x0F;
    let algorithm = match algorithm_raw {
        1 => AntminerEepromAlgorithm::Xxtea,
        2 => AntminerEepromAlgorithm::Xor,
        other => AntminerEepromAlgorithm::Unknown(other),
    };

    if usize::from(key_index) >= KEY_LARGE.len() {
        return Err(format!("unsupported EEPROM key index: {key_index}"));
    }

    let mut decoded = data.to_vec();
    decode_region(&mut decoded[REGION1_START..REGION1_START + REGION1_SIZE], algorithm, key_index)?;
    decode_region(&mut decoded[REGION2_START..REGION2_START + REGION2_SIZE], algorithm, key_index)?;
    if matches!(version, AntminerEepromVersion::V5 | AntminerEepromVersion::V6) {
        decode_region(&mut decoded[REGION3_START..REGION3_START + REGION3_SIZE], algorithm, key_index)?;
    }

    let board_serial = decode_string(&decoded[2..20]);
    let chip_die = decode_string(&decoded[20..23]);
    let chip_marking = decode_string(&decoded[23..37]);
    let chip_bin = decoded[37];
    let ft_version = decode_string(&decoded[38..48]);
    let pcb_version = u16::from_le_bytes([decoded[48], decoded[49]]);
    let bom_version = u16::from_le_bytes([decoded[50], decoded[51]]);
    let asic_sensor_type = decoded[52];
    let asic_sensor_addr = [decoded[53], decoded[54], decoded[55], decoded[56]];
    let pic_sensor_type = decoded[57];
    let pic_sensor_addr = decoded[58];
    let chip_tech = decode_string(&decoded[59..62]);
    let board_name = decode_string(&decoded[62..71]);
    let factory_job = decode_string(&decoded[71..95]);
    let pt1_result = decoded[95];
    let pt1_count = decoded[96];
    let pt1_crc_stored = decoded[97];
    let pt1_crc_calculated = calculate_crc(&decoded[..REGION1_CRC_POS], REGION1_CRC_BITS);

    let voltage_cv = u16::from_le_bytes([decoded[98], decoded[99]]);
    let frequency_mhz = u16::from_le_bytes([decoded[100], decoded[101]]);
    let nonce_rate = u16::from_le_bytes([decoded[102], decoded[103]]);
    let pcb_temp_in_c = decoded[104] as i8;
    let pcb_temp_out_c = decoded[105] as i8;
    let test_version = decoded[106];
    let test_standard = decoded[107];
    let pt2_result = decoded[108];
    let pt2_count = decoded[109];
    let pt2_crc_stored = decoded[113];
    let pt2_crc_calculated_tool = calculate_crc(&decoded[..15], 15 * 8);
    let pt2_crc_calculated_region = calculate_crc(&decoded[REGION2_START..REGION2_CRC_POS], 15 * 8);

    let (sweep_hashrate, sweep_freq_base, sweep_freq_step, sweep_result) = match version {
        AntminerEepromVersion::V4 => (None, None, None, None),
        AntminerEepromVersion::V5 | AntminerEepromVersion::V6 => (
            Some(u16::from_le_bytes([decoded[114], decoded[115]])),
            Some(u16::from_le_bytes([decoded[116], decoded[117]])),
            Some(decoded[118]),
            Some(decoded[247]),
        ),
    };

    let sweep_slice = &decoded[REGION3_START..];
    let sweep_non_ff = sweep_slice.iter().filter(|&&byte| byte != 0xFF).count();
    let sweep_prefix_len = sweep_slice.iter().position(|&byte| byte == 0xFF).unwrap_or(sweep_slice.len());
    let sweep_prefix = sweep_slice[..sweep_prefix_len].to_vec();

    Ok(DecodedAntminerEeprom {
        version,
        algorithm,
        key_index,
        decoded_bytes: decoded,
        board_serial,
        chip_die,
        chip_marking,
        chip_bin,
        ft_version,
        pcb_version,
        bom_version,
        asic_sensor_type,
        asic_sensor_addr,
        pic_sensor_type,
        pic_sensor_addr,
        chip_tech,
        board_name,
        factory_job,
        pt1_result,
        pt1_count,
        pt1_crc_stored,
        pt1_crc_calculated,
        voltage_cv,
        frequency_mhz,
        nonce_rate,
        pcb_temp_in_c,
        pcb_temp_out_c,
        test_version,
        test_standard,
        pt2_result,
        pt2_count,
        pt2_crc_stored,
        pt2_crc_calculated_tool,
        pt2_crc_calculated_region,
        sweep_hashrate,
        sweep_freq_base,
        sweep_freq_step,
        sweep_result,
        sweep_non_ff,
        sweep_prefix,
    })
}

fn decode_region(region: &mut [u8], algorithm: AntminerEepromAlgorithm, key_index: u8) -> Result<(), String> {
    match algorithm {
        AntminerEepromAlgorithm::Xxtea => {
            xxtea_decode(region, &KEY_LARGE[usize::from(key_index)]);
            Ok(())
        }
        AntminerEepromAlgorithm::Xor => {
            xor_decode(region, KEY_SMALL[usize::from(key_index)]);
            Ok(())
        }
        AntminerEepromAlgorithm::Unknown(value) => Err(format!("unsupported EEPROM algorithm: {value}")),
    }
}

fn xor_decode(data: &mut [u8], key: u32) {
    for chunk in data.chunks_exact_mut(4) {
        let value = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) ^ key;
        chunk.copy_from_slice(&value.to_le_bytes());
    }
}

fn xxtea_decode(data: &mut [u8], key_bytes: &[u8; 16]) {
    let n = data.len() / 4;
    if n < 2 {
        return;
    }

    let mut v = Vec::with_capacity(n);
    for chunk in data.chunks_exact(4) {
        v.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }

    let mut k = [0u32; 4];
    for (index, chunk) in key_bytes.chunks_exact(4).enumerate() {
        k[index] = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }

    let rounds = 6u32 + 52u32 / n as u32;
    let mut sum = rounds.wrapping_mul(DELTA);
    let mut y = v[0];

    while sum != 0 {
        let e = (sum >> 2) & 3;
        for p in (1..n).rev() {
            let z = v[p - 1];
            y = v[p].wrapping_sub(mx(sum, y, z, p as u32, e, &k));
            v[p] = y;
        }

        let z = v[n - 1];
        y = v[0].wrapping_sub(mx(sum, y, z, 0, e, &k));
        v[0] = y;
        sum = sum.wrapping_sub(DELTA);
    }

    for (chunk, value) in data.chunks_exact_mut(4).zip(v.into_iter()) {
        chunk.copy_from_slice(&value.to_le_bytes());
    }
}

fn mx(sum: u32, y: u32, z: u32, p: u32, e: u32, key: &[u32; 4]) -> u32 {
    ((z >> 5 ^ y << 2).wrapping_add(y >> 3 ^ z << 4))
        ^ ((sum ^ y).wrapping_add(key[((p & 3) ^ e) as usize] ^ z))
}

fn calculate_crc(bytes: &[u8], bits: usize) -> u8 {
    let mut crc = 0xFFu8 << 3;
    let mut index = 0usize;
    let byte_count = bits >> 3;

    while index < byte_count {
        crc = CRC5_LOOKUP[usize::from(crc ^ bytes[index])];
        index += 1;
    }

    let trailing_bits = bits & 7;
    if trailing_bits != 0 && index < bytes.len() {
        crc = (crc << trailing_bits) ^ CRC5_LOOKUP[usize::from((crc ^ bytes[index]) >> (8 - trailing_bits))];
    }

    crc >> 3
}

fn decode_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&byte| byte == 0).unwrap_or(bytes.len());
    bytes[..end]
        .iter()
        .map(|&byte| if byte.is_ascii_graphic() || byte == b' ' { byte as char } else { '?' })
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(hex_data: &str) -> DecodedAntminerEeprom {
        let mut bytes = hex::decode(hex_data).unwrap();
        bytes.resize(EEPROM_SIZE, 0xFF);
        decode_antminer_eeprom(&bytes).unwrap()
    }

    #[test]
    fn decodes_hb0_sample() {
        let decoded = decode("0411193af9a05572cae5d54209b90d41a09dee5608bf7d1cb123a247acdf1470b5f4450d43fd5efdec34d0c929f37d011e273692691d4613fa11933628472f326f541c861792e5899787b1d1f84f1c42c443681e8869445556805019c862f54876f9c2712bbe0af4eb97df0126f5650b30baffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff5a");
        assert_eq!(decoded.board_serial, "KPMIYNRBBJDBH1985");
        assert_eq!(decoded.board_name, "BHB42603");
        assert_eq!(decoded.factory_job, "KPMI20220401001");
        assert_eq!(decoded.frequency_mhz, 525);
        assert_eq!(decoded.voltage_cv, 1380);
        assert_eq!(decoded.nonce_rate, 9989);
    }

    #[test]
    fn decodes_hb1_sample() {
        let decoded = decode("041179627d9cd881437f5d219879af1217a74af3af9178824400638bc6a9cf2d37d4ad326faaddc2b7888dda33fef1387ad04717099204598304192cee201125510b0624523e135d8752e2d31341ad0b6cf9a78cf26ca843c61d61635ac936a0bf8a0e27e60d533e06bd63d84d4bc267e554ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff5a");
        assert_eq!(decoded.board_serial, "KPMIYNRBBJDBI0792");
        assert_eq!(decoded.chip_bin, 2);
        assert_eq!(decoded.nonce_rate, 9998);
    }

    #[test]
    fn decodes_hb2_sample() {
        let decoded = decode("04115ac38425641a12ee2611b311e2e3612708350201d57f5034c11ef7c4ba42c6bb59cf422630546ed11aede669f53c58bc2b4fae17980cd3fdf26ff739f82097d06a5e12fccdcaac2e91df886e68c4bcb26e755436ee609bcec3f93bd8a6c640d1ff8fed56f44bf110495e1f6d39a4bcb1ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff5a");
        assert_eq!(decoded.board_serial, "KPMIYNRBBJDAE0701");
        assert_eq!(decoded.voltage_cv, 1360);
        assert_eq!(decoded.pcb_temp_in_c, 25);
        assert_eq!(decoded.pcb_temp_out_c, 28);
    }
}