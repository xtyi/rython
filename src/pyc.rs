use crate::code::CodeObject;
use crate::error::{ParseError, Result};
use crate::marshal::{parse_marshal_with_limits, MarshalLimits, MarshalValue};

pub const PYTHON_3_12_MAGIC: [u8; 4] = [0xcb, 0x0d, 0x0d, 0x0a];
const PYC_HEADER_SIZE: usize = 16;
const VALID_FLAGS: u32 = 0b11;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PycValidation {
    Timestamp { timestamp: u32, source_size: u32 },
    Hash { hash: [u8; 8], check_source: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PycHeader {
    pub magic: [u8; 4],
    pub flags: u32,
    pub validation: PycValidation,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PycFile {
    pub header: PycHeader,
    pub code: CodeObject,
}

pub fn parse_header(input: &[u8]) -> Result<(PycHeader, usize)> {
    if input.len() < PYC_HEADER_SIZE {
        return Err(ParseError::UnexpectedEof {
            offset: input.len(),
            needed: PYC_HEADER_SIZE - input.len(),
            remaining: 0,
        });
    }

    let actual = input[0..4].try_into().expect("length checked");
    if actual != PYTHON_3_12_MAGIC {
        return Err(ParseError::InvalidMagic {
            expected: PYTHON_3_12_MAGIC,
            actual,
        });
    }

    let flags = u32::from_le_bytes(input[4..8].try_into().expect("length checked"));
    if flags & !VALID_FLAGS != 0 {
        return Err(ParseError::InvalidHeaderFlags(flags));
    }

    let validation = if flags & 1 == 0 {
        PycValidation::Timestamp {
            timestamp: u32::from_le_bytes(input[8..12].try_into().expect("length checked")),
            source_size: u32::from_le_bytes(input[12..16].try_into().expect("length checked")),
        }
    } else {
        PycValidation::Hash {
            hash: input[8..16].try_into().expect("length checked"),
            check_source: flags & 2 != 0,
        }
    };

    Ok((
        PycHeader {
            magic: actual,
            flags,
            validation,
        },
        PYC_HEADER_SIZE,
    ))
}

pub fn parse_pyc(input: &[u8]) -> Result<PycFile> {
    parse_pyc_with_limits(input, MarshalLimits::default())
}

pub(crate) fn parse_pyc_with_limits(input: &[u8], limits: MarshalLimits) -> Result<PycFile> {
    let (header, payload_offset) = parse_header(input)?;
    let root = parse_marshal_with_limits(&input[payload_offset..], limits)?;
    let code = match root {
        MarshalValue::Code(code) => *code,
        _ => return Err(ParseError::RootIsNotCodeObject),
    };
    Ok(PycFile { header, code })
}
