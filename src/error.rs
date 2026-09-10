use std::fmt;

pub type Result<T> = std::result::Result<T, ParseError>;
pub type BytecodeResult<T> = std::result::Result<T, BytecodeError>;
pub type VmResult<T> = std::result::Result<T, VmError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    UnexpectedEof {
        offset: usize,
        needed: usize,
        remaining: usize,
    },
    InvalidMagic {
        expected: [u8; 4],
        actual: [u8; 4],
    },
    InvalidHeaderFlags(u32),
    UnsupportedMarshalType {
        offset: usize,
        tag: u8,
    },
    InvalidReference {
        offset: usize,
        index: i32,
        count: usize,
    },
    RecursiveReference {
        offset: usize,
        index: i32,
    },
    InvalidUtf8 {
        offset: usize,
    },
    InvalidFloat {
        offset: usize,
    },
    InvalidLength {
        offset: usize,
        length: i64,
    },
    ResourceLimit {
        offset: usize,
        what: &'static str,
        limit: usize,
    },
    InvalidCodeField {
        field: &'static str,
        expected: &'static str,
    },
    InvalidCodeObject(&'static str),
    RootIsNotCodeObject,
    TrailingBytes {
        offset: usize,
        remaining: usize,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof {
                offset,
                needed,
                remaining,
            } => write!(
                f,
                "unexpected end of input at offset {offset}: needed {needed} bytes, \
                 found {remaining}"
            ),
            Self::InvalidMagic { expected, actual } => {
                write!(
                    f,
                    "invalid .pyc magic: expected {expected:02x?}, got {actual:02x?}"
                )
            }
            Self::InvalidHeaderFlags(flags) => {
                write!(f, "invalid .pyc header flags: 0x{flags:08x}")
            }
            Self::UnsupportedMarshalType { offset, tag } => {
                write!(f, "unsupported marshal type 0x{tag:02x} at offset {offset}")
            }
            Self::InvalidReference {
                offset,
                index,
                count,
            } => write!(
                f,
                "invalid marshal reference {index} at offset {offset}; \
                 reference table has {count} entries"
            ),
            Self::RecursiveReference { offset, index } => write!(
                f,
                "recursive marshal reference {index} at offset {offset} is not \
                 supported by the owned parser representation"
            ),
            Self::InvalidUtf8 { offset } => {
                write!(f, "invalid UTF-8 string at offset {offset}")
            }
            Self::InvalidFloat { offset } => {
                write!(f, "invalid floating-point value at offset {offset}")
            }
            Self::InvalidLength { offset, length } => {
                write!(f, "invalid length {length} at offset {offset}")
            }
            Self::ResourceLimit {
                offset,
                what,
                limit,
            } => write!(
                f,
                "marshal {what} exceeds the limit {limit} at offset {offset}"
            ),
            Self::InvalidCodeField { field, expected } => {
                write!(f, "invalid code object field {field}; expected {expected}")
            }
            Self::InvalidCodeObject(reason) => write!(f, "invalid code object: {reason}"),
            Self::RootIsNotCodeObject => write!(f, "marshal root is not a code object"),
            Self::TrailingBytes { offset, remaining } => write!(
                f,
                "trailing bytes after marshal root at offset {offset}: {remaining} bytes"
            ),
        }
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BytecodeError {
    OddLength { length: usize },
    TruncatedInstruction { offset: usize },
    DanglingExtendedArg { offset: usize },
    ExtendedArgOverflow { offset: usize },
}

impl fmt::Display for BytecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OddLength { length } => {
                write!(f, "Python 3.12 bytecode has odd length {length}")
            }
            Self::TruncatedInstruction { offset } => {
                write!(f, "truncated bytecode instruction at offset {offset}")
            }
            Self::DanglingExtendedArg { offset } => {
                write!(
                    f,
                    "EXTENDED_ARG at offset {offset} has no following instruction"
                )
            }
            Self::ExtendedArgOverflow { offset } => {
                write!(f, "EXTENDED_ARG sequence overflows at offset {offset}")
            }
        }
    }
}

impl std::error::Error for BytecodeError {}

#[derive(Debug, Clone, PartialEq)]
pub enum VmError {
    Bytecode(BytecodeError),
    UnsupportedOpcode {
        offset: usize,
        opcode: u8,
    },
    UnsupportedOperation {
        offset: usize,
        operation: String,
    },
    InvalidArgument {
        offset: usize,
        opcode: u8,
        argument: u32,
        reason: String,
    },
    StackUnderflow {
        offset: usize,
        opcode: u8,
    },
    MissingReturn {
        code_name: String,
    },
    ConstantIndexOutOfBounds {
        offset: usize,
        index: u32,
    },
    NameIndexOutOfBounds {
        offset: usize,
        index: u32,
    },
    LocalIndexOutOfBounds {
        offset: usize,
        index: u32,
    },
    UnknownName {
        offset: usize,
        name: String,
    },
    UnboundLocal {
        offset: usize,
        name: String,
    },
    UnsupportedConstant {
        description: String,
    },
    TypeError {
        offset: usize,
        operation: String,
        left: String,
        right: Option<String>,
    },
    ValueError {
        offset: usize,
        operation: String,
    },
    ZeroDivision {
        offset: usize,
    },
    IntegerOverflow {
        offset: usize,
        operation: String,
    },
    IndexError {
        offset: usize,
        index: i64,
    },
    KeyError {
        offset: usize,
    },
    InvalidJumpTarget {
        offset: usize,
        target: usize,
    },
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bytecode(error) => error.fmt(f),
            Self::UnsupportedOpcode { offset, opcode } => {
                write!(f, "unsupported opcode 0x{opcode:02x} at offset {offset}")
            }
            Self::UnsupportedOperation { offset, operation } => {
                write!(f, "unsupported operation {operation} at offset {offset}")
            }
            Self::InvalidArgument {
                offset,
                opcode,
                argument,
                reason,
            } => write!(
                f,
                "invalid argument {argument} for opcode 0x{opcode:02x} at offset {offset}: {reason}"
            ),
            Self::StackUnderflow { offset, opcode } => {
                write!(
                    f,
                    "stack underflow for opcode 0x{opcode:02x} at offset {offset}"
                )
            }
            Self::MissingReturn { code_name } => {
                write!(f, "code object {code_name:?} reached end without returning")
            }
            Self::ConstantIndexOutOfBounds { offset, index } => {
                write!(
                    f,
                    "constant index {index} is out of bounds at offset {offset}"
                )
            }
            Self::NameIndexOutOfBounds { offset, index } => {
                write!(f, "name index {index} is out of bounds at offset {offset}")
            }
            Self::LocalIndexOutOfBounds { offset, index } => {
                write!(f, "local index {index} is out of bounds at offset {offset}")
            }
            Self::UnknownName { offset, name } => {
                write!(f, "name {name:?} is not defined at offset {offset}")
            }
            Self::UnboundLocal { offset, name } => {
                write!(f, "local {name:?} is unbound at offset {offset}")
            }
            Self::UnsupportedConstant { description } => {
                write!(f, "unsupported Python constant: {description}")
            }
            Self::TypeError {
                offset,
                operation,
                left,
                right,
            } => {
                if let Some(right) = right {
                    write!(
                        f,
                        "unsupported operand types for {operation}: {left} and {right} \
                         at offset {offset}"
                    )
                } else {
                    write!(
                        f,
                        "unsupported operand type for {operation}: {left} at offset {offset}"
                    )
                }
            }
            Self::ValueError { offset, operation } => {
                write!(f, "invalid value for {operation} at offset {offset}")
            }
            Self::ZeroDivision { offset } => {
                write!(f, "division by zero at offset {offset}")
            }
            Self::IntegerOverflow { offset, operation } => {
                write!(f, "integer overflow during {operation} at offset {offset}")
            }
            Self::IndexError { offset, index } => {
                write!(f, "index {index} is out of range at offset {offset}")
            }
            Self::KeyError { offset } => write!(f, "key was not found at offset {offset}"),
            Self::InvalidJumpTarget { offset, target } => {
                write!(f, "invalid jump target {target} from offset {offset}")
            }
        }
    }
}

impl std::error::Error for VmError {}

impl From<BytecodeError> for VmError {
    fn from(error: BytecodeError) -> Self {
        Self::Bytecode(error)
    }
}
