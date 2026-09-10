use crate::code::CodeObject;
use crate::error::{ParseError, Result};

const FLAG_REF: u8 = 0x80;

const TYPE_NULL: u8 = b'0';
const TYPE_NONE: u8 = b'N';
const TYPE_FALSE: u8 = b'F';
const TYPE_TRUE: u8 = b'T';
const TYPE_STOP_ITERATOR: u8 = b'S';
const TYPE_ELLIPSIS: u8 = b'.';
const TYPE_INT: u8 = b'i';
const TYPE_INT64: u8 = b'I';
const TYPE_FLOAT: u8 = b'f';
const TYPE_BINARY_FLOAT: u8 = b'g';
const TYPE_COMPLEX: u8 = b'x';
const TYPE_BINARY_COMPLEX: u8 = b'y';
const TYPE_LONG: u8 = b'l';
const TYPE_STRING: u8 = b's';
const TYPE_INTERNED: u8 = b't';
const TYPE_REF: u8 = b'r';
const TYPE_TUPLE: u8 = b'(';
const TYPE_LIST: u8 = b'[';
const TYPE_DICT: u8 = b'{';
const TYPE_CODE: u8 = b'c';
const TYPE_UNICODE: u8 = b'u';
const TYPE_UNKNOWN: u8 = b'?';
const TYPE_SET: u8 = b'<';
const TYPE_FROZENSET: u8 = b'>';
const TYPE_ASCII: u8 = b'a';
const TYPE_ASCII_INTERNED: u8 = b'A';
const TYPE_SMALL_TUPLE: u8 = b')';
const TYPE_SHORT_ASCII: u8 = b'z';
const TYPE_SHORT_ASCII_INTERNED: u8 = b'Z';

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarshalLong {
    pub negative: bool,
    pub digits: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MarshalValue {
    None,
    Bool(bool),
    StopIteration,
    Ellipsis,
    Int(i64),
    Long(MarshalLong),
    Float(f64),
    Complex { real: f64, imaginary: f64 },
    Bytes(Vec<u8>),
    String(String),
    Tuple(Vec<MarshalValue>),
    List(Vec<MarshalValue>),
    Dict(Vec<(MarshalValue, MarshalValue)>),
    Set(Vec<MarshalValue>),
    FrozenSet(Vec<MarshalValue>),
    Code(Box<CodeObject>),
}

#[derive(Debug, Clone, Copy)]
pub struct MarshalLimits {
    pub max_depth: usize,
    pub max_container_length: usize,
    pub max_input_bytes: usize,
}

impl Default for MarshalLimits {
    fn default() -> Self {
        Self {
            max_depth: 256,
            max_container_length: 16 * 1024 * 1024,
            max_input_bytes: 64 * 1024 * 1024,
        }
    }
}

pub fn parse_marshal(input: &[u8]) -> Result<MarshalValue> {
    parse_marshal_with_limits(input, MarshalLimits::default())
}

pub fn parse_code_object(input: &[u8]) -> Result<CodeObject> {
    match parse_marshal(input)? {
        MarshalValue::Code(code) => Ok(*code),
        _ => Err(ParseError::RootIsNotCodeObject),
    }
}

pub(crate) fn parse_marshal_with_limits(
    input: &[u8],
    limits: MarshalLimits,
) -> Result<MarshalValue> {
    if input.len() > limits.max_input_bytes {
        return Err(ParseError::ResourceLimit {
            offset: 0,
            what: "input size",
            limit: limits.max_input_bytes,
        });
    }
    let mut reader = MarshalReader::new(input, limits);
    let value = reader.read_object(0)?;
    if reader.remaining() != 0 {
        return Err(ParseError::TrailingBytes {
            offset: reader.offset(),
            remaining: reader.remaining(),
        });
    }
    Ok(value)
}

struct ByteReader<'a> {
    input: &'a [u8],
    offset: usize,
}

impl<'a> ByteReader<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self { input, offset: 0 }
    }

    fn offset(&self) -> usize {
        self.offset
    }

    fn remaining(&self) -> usize {
        self.input.len() - self.offset
    }

    fn peek_u8(&self) -> Option<u8> {
        self.input.get(self.offset).copied()
    }

    fn read_u8(&mut self) -> Result<u8> {
        if let Some(value) = self.input.get(self.offset).copied() {
            self.offset += 1;
            Ok(value)
        } else {
            Err(ParseError::UnexpectedEof {
                offset: self.offset,
                needed: 1,
                remaining: 0,
            })
        }
    }

    fn read_exact(&mut self, length: usize) -> Result<&'a [u8]> {
        let remaining = self.remaining();
        if remaining < length {
            return Err(ParseError::UnexpectedEof {
                offset: self.offset,
                needed: length,
                remaining,
            });
        }
        let start = self.offset;
        self.offset += length;
        Ok(&self.input[start..self.offset])
    }

    fn read_i32(&mut self) -> Result<i32> {
        let bytes = self.read_exact(4)?;
        Ok(i32::from_le_bytes(
            bytes.try_into().expect("length checked"),
        ))
    }

    fn read_i64(&mut self) -> Result<i64> {
        let bytes = self.read_exact(8)?;
        Ok(i64::from_le_bytes(
            bytes.try_into().expect("length checked"),
        ))
    }

    fn read_i16(&mut self) -> Result<i16> {
        let bytes = self.read_exact(2)?;
        Ok(i16::from_le_bytes(
            bytes.try_into().expect("length checked"),
        ))
    }

    fn read_f64_le(&mut self) -> Result<f64> {
        let bytes = self.read_exact(8)?;
        Ok(f64::from_le_bytes(
            bytes.try_into().expect("length checked"),
        ))
    }
}

struct MarshalReader<'a> {
    reader: ByteReader<'a>,
    limits: MarshalLimits,
    references: Vec<Option<MarshalValue>>,
}

impl<'a> MarshalReader<'a> {
    fn new(input: &'a [u8], limits: MarshalLimits) -> Self {
        Self {
            reader: ByteReader::new(input),
            limits,
            references: Vec::new(),
        }
    }

    fn offset(&self) -> usize {
        self.reader.offset()
    }

    fn remaining(&self) -> usize {
        self.reader.remaining()
    }

    fn read_object(&mut self, depth: usize) -> Result<MarshalValue> {
        if depth > self.limits.max_depth {
            return Err(ParseError::ResourceLimit {
                offset: self.offset(),
                what: "nesting depth",
                limit: self.limits.max_depth,
            });
        }

        let tag_offset = self.offset();
        let raw_tag = self.reader.read_u8()?;
        let has_reference = raw_tag & FLAG_REF != 0;
        let tag = raw_tag & !FLAG_REF;

        if tag == TYPE_REF {
            if has_reference {
                return Err(ParseError::UnsupportedMarshalType {
                    offset: tag_offset,
                    tag: raw_tag,
                });
            }
            return self.read_reference();
        }

        if tag == TYPE_NULL {
            return Err(ParseError::UnsupportedMarshalType {
                offset: tag_offset,
                tag: raw_tag,
            });
        }

        let reference_index = if has_reference {
            let index = self.references.len();
            self.references.push(None);
            Some(index)
        } else {
            None
        };

        let value = self.read_object_body(tag, depth + 1)?;
        if let Some(index) = reference_index {
            self.references[index] = Some(value.clone());
        }
        Ok(value)
    }

    fn read_reference(&mut self) -> Result<MarshalValue> {
        let offset = self.offset();
        let index = self.reader.read_i32()?;
        if index < 0 {
            return Err(ParseError::InvalidReference {
                offset,
                index,
                count: self.references.len(),
            });
        }
        let index_usize = index as usize;
        match self.references.get(index_usize) {
            Some(Some(value)) => Ok(value.clone()),
            Some(None) => Err(ParseError::RecursiveReference { offset, index }),
            None => Err(ParseError::InvalidReference {
                offset,
                index,
                count: self.references.len(),
            }),
        }
    }

    fn read_object_body(&mut self, tag: u8, depth: usize) -> Result<MarshalValue> {
        match tag {
            TYPE_NONE => Ok(MarshalValue::None),
            TYPE_FALSE => Ok(MarshalValue::Bool(false)),
            TYPE_TRUE => Ok(MarshalValue::Bool(true)),
            TYPE_STOP_ITERATOR => Ok(MarshalValue::StopIteration),
            TYPE_ELLIPSIS => Ok(MarshalValue::Ellipsis),
            TYPE_INT => Ok(MarshalValue::Int(self.reader.read_i32()? as i64)),
            TYPE_INT64 => Ok(MarshalValue::Int(self.reader.read_i64()?)),
            TYPE_LONG => self.read_long(),
            TYPE_FLOAT => self.read_decimal_float(),
            TYPE_BINARY_FLOAT => Ok(MarshalValue::Float(self.reader.read_f64_le()?)),
            TYPE_COMPLEX => {
                let real = self.read_decimal_float_value()?;
                let imaginary = self.read_decimal_float_value()?;
                Ok(MarshalValue::Complex { real, imaginary })
            }
            TYPE_BINARY_COMPLEX => {
                let real = self.reader.read_f64_le()?;
                let imaginary = self.reader.read_f64_le()?;
                Ok(MarshalValue::Complex { real, imaginary })
            }
            TYPE_STRING => Ok(MarshalValue::Bytes(self.read_length_prefixed_bytes()?)),
            TYPE_INTERNED | TYPE_UNICODE => {
                Ok(MarshalValue::String(self.read_length_prefixed_string()?))
            }
            TYPE_ASCII | TYPE_ASCII_INTERNED => {
                let length = self.read_length()?;
                Ok(MarshalValue::String(self.read_ascii_string(length)?))
            }
            TYPE_SHORT_ASCII | TYPE_SHORT_ASCII_INTERNED => {
                let length = self.read_short_length()?;
                Ok(MarshalValue::String(self.read_ascii_string(length)?))
            }
            TYPE_TUPLE => {
                let length = self.read_length()?;
                self.read_sequence(depth, SequenceKind::Tuple, length)
            }
            TYPE_SMALL_TUPLE => {
                let length = self.read_short_length()?;
                self.read_sequence(depth, SequenceKind::Tuple, length)
            }
            TYPE_LIST => {
                let length = self.read_length()?;
                self.read_sequence(depth, SequenceKind::List, length)
            }
            TYPE_SET => {
                let length = self.read_length()?;
                self.read_sequence(depth, SequenceKind::Set, length)
            }
            TYPE_FROZENSET => {
                let length = self.read_length()?;
                self.read_sequence(depth, SequenceKind::FrozenSet, length)
            }
            TYPE_DICT => self.read_dict(depth),
            TYPE_CODE => self.read_code(depth),
            TYPE_UNKNOWN => Err(ParseError::UnsupportedMarshalType {
                offset: self.offset().saturating_sub(1),
                tag,
            }),
            _ => Err(ParseError::UnsupportedMarshalType {
                offset: self.offset().saturating_sub(1),
                tag,
            }),
        }
    }

    fn read_length(&mut self) -> Result<usize> {
        let offset = self.offset();
        let length = self.reader.read_i32()? as i64;
        self.validate_length(offset, length)
    }

    fn read_short_length(&mut self) -> Result<usize> {
        let offset = self.offset();
        let length = self.reader.read_u8()? as i64;
        self.validate_length(offset, length)
    }

    fn validate_length(&self, offset: usize, length: i64) -> Result<usize> {
        if length < 0 {
            return Err(ParseError::InvalidLength { offset, length });
        }
        let length = length as usize;
        if length > self.limits.max_container_length {
            return Err(ParseError::ResourceLimit {
                offset,
                what: "container length",
                limit: self.limits.max_container_length,
            });
        }
        Ok(length)
    }

    fn read_length_prefixed_bytes(&mut self) -> Result<Vec<u8>> {
        let length = self.read_length()?;
        Ok(self.reader.read_exact(length)?.to_vec())
    }

    fn read_length_prefixed_string(&mut self) -> Result<String> {
        let offset = self.offset();
        let bytes = self.read_length_prefixed_bytes()?;
        String::from_utf8(bytes).map_err(|_| ParseError::InvalidUtf8 { offset })
    }

    fn read_ascii_string(&mut self, length: usize) -> Result<String> {
        let offset = self.offset();
        let bytes = self.reader.read_exact(length)?;
        if !bytes.is_ascii() {
            return Err(ParseError::InvalidUtf8 { offset });
        }
        Ok(String::from_utf8(bytes.to_vec()).expect("ASCII is valid UTF-8"))
    }

    fn read_decimal_float(&mut self) -> Result<MarshalValue> {
        Ok(MarshalValue::Float(self.read_decimal_float_value()?))
    }

    fn read_decimal_float_value(&mut self) -> Result<f64> {
        let offset = self.offset();
        let length = self.reader.read_u8()? as usize;
        if length > self.limits.max_container_length {
            return Err(ParseError::ResourceLimit {
                offset,
                what: "float string length",
                limit: self.limits.max_container_length,
            });
        }
        let bytes = self.reader.read_exact(length)?;
        let text = std::str::from_utf8(bytes).map_err(|_| ParseError::InvalidUtf8 { offset })?;
        text.parse::<f64>()
            .map_err(|_| ParseError::InvalidFloat { offset })
    }

    fn read_long(&mut self) -> Result<MarshalValue> {
        let offset = self.offset();
        let raw_count = self.reader.read_i32()? as i64;
        let negative = raw_count < 0;
        let count = raw_count.unsigned_abs() as usize;
        if count > self.limits.max_container_length {
            return Err(ParseError::ResourceLimit {
                offset,
                what: "long integer digit count",
                limit: self.limits.max_container_length,
            });
        }

        let mut digits = Vec::with_capacity(count);
        for _ in 0..count {
            let digit = self.reader.read_i16()?;
            if digit < 0 {
                return Err(ParseError::InvalidCodeObject(
                    "marshal long digit is outside the base-2^15 range",
                ));
            }
            digits.push(digit as u16);
        }
        Ok(MarshalValue::Long(MarshalLong { negative, digits }))
    }

    fn read_sequence(
        &mut self,
        depth: usize,
        kind: SequenceKind,
        length: usize,
    ) -> Result<MarshalValue> {
        let mut values = Vec::with_capacity(length);
        for _ in 0..length {
            values.push(self.read_object(depth)?);
        }
        Ok(match kind {
            SequenceKind::Tuple => MarshalValue::Tuple(values),
            SequenceKind::List => MarshalValue::List(values),
            SequenceKind::Set => MarshalValue::Set(values),
            SequenceKind::FrozenSet => MarshalValue::FrozenSet(values),
        })
    }

    fn read_dict(&mut self, depth: usize) -> Result<MarshalValue> {
        let mut entries = Vec::new();
        loop {
            if self.reader.peek_u8() == Some(TYPE_NULL) {
                self.reader.read_u8()?;
                break;
            }
            if entries.len() >= self.limits.max_container_length {
                return Err(ParseError::ResourceLimit {
                    offset: self.offset(),
                    what: "dictionary length",
                    limit: self.limits.max_container_length,
                });
            }
            let key = self.read_object(depth)?;
            let value = self.read_object(depth)?;
            entries.push((key, value));
        }
        Ok(MarshalValue::Dict(entries))
    }

    fn read_code(&mut self, depth: usize) -> Result<MarshalValue> {
        let arg_count = self.read_code_u32("arg_count")?;
        let positional_only_arg_count = self.read_code_u32("positional_only_arg_count")?;
        let keyword_only_arg_count = self.read_code_u32("keyword_only_arg_count")?;
        let stack_size = self.read_code_u32("stack_size")?;
        let flags = self.read_code_u32("flags")?;

        let value = self.read_object(depth)?;
        let bytecode = Self::expect_bytes(value, "bytecode")?;
        let value = self.read_object(depth)?;
        let constants = Self::expect_tuple(value, "constants")?;
        let value = self.read_object(depth)?;
        let names = Self::expect_string_tuple(value, "names")?;
        let value = self.read_object(depth)?;
        let localsplus_names = Self::expect_string_tuple(value, "localsplus_names")?;
        let value = self.read_object(depth)?;
        let localsplus_kinds = Self::expect_bytes(value, "localsplus_kinds")?;
        let value = self.read_object(depth)?;
        let filename = Self::expect_string(value, "filename")?;
        let value = self.read_object(depth)?;
        let name = Self::expect_string(value, "name")?;
        let value = self.read_object(depth)?;
        let qualified_name = Self::expect_string(value, "qualified_name")?;
        let first_line_number = self.read_code_u32("first_line_number")?;
        let value = self.read_object(depth)?;
        let line_table = Self::expect_bytes(value, "line_table")?;
        let value = self.read_object(depth)?;
        let exception_table = Self::expect_bytes(value, "exception_table")?;

        if localsplus_names.len() != localsplus_kinds.len() {
            return Err(ParseError::InvalidCodeObject(
                "localsplus_names and localsplus_kinds have different lengths",
            ));
        }

        Ok(MarshalValue::Code(Box::new(CodeObject {
            arg_count,
            positional_only_arg_count,
            keyword_only_arg_count,
            stack_size,
            flags,
            bytecode,
            constants,
            names,
            localsplus_names,
            localsplus_kinds,
            filename,
            name,
            qualified_name,
            first_line_number,
            line_table,
            exception_table,
        })))
    }

    fn read_code_u32(&mut self, field: &'static str) -> Result<u32> {
        let value = self.reader.read_i32()?;
        if value < 0 {
            return Err(ParseError::InvalidCodeObject(field));
        }
        Ok(value as u32)
    }

    fn expect_bytes(value: MarshalValue, field: &'static str) -> Result<Vec<u8>> {
        match value {
            MarshalValue::Bytes(bytes) => Ok(bytes),
            _ => Err(ParseError::InvalidCodeField {
                field,
                expected: "bytes",
            }),
        }
    }

    fn expect_string(value: MarshalValue, field: &'static str) -> Result<String> {
        match value {
            MarshalValue::String(string) => Ok(string),
            _ => Err(ParseError::InvalidCodeField {
                field,
                expected: "string",
            }),
        }
    }

    fn expect_tuple(value: MarshalValue, field: &'static str) -> Result<Vec<MarshalValue>> {
        match value {
            MarshalValue::Tuple(values) => Ok(values),
            _ => Err(ParseError::InvalidCodeField {
                field,
                expected: "tuple",
            }),
        }
    }

    fn expect_string_tuple(value: MarshalValue, field: &'static str) -> Result<Vec<String>> {
        let values = Self::expect_tuple(value, field)?;
        values
            .into_iter()
            .map(|value| Self::expect_string(value, field))
            .collect()
    }
}

#[derive(Clone, Copy)]
enum SequenceKind {
    Tuple,
    List,
    Set,
    FrozenSet,
}
