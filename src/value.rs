use crate::error::{VmError, VmResult};
use crate::marshal::{MarshalLong, MarshalValue};
use crate::CodeObject;

#[derive(Debug, Clone, PartialEq)]
pub enum VmValue {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Tuple(Vec<VmValue>),
    List(Vec<VmValue>),
    Set(Vec<VmValue>),
    FrozenSet(Vec<VmValue>),
    Dict(Vec<(VmValue, VmValue)>),
    Code(Box<CodeObject>),
    BuiltinPrint,
}

impl VmValue {
    pub fn from_marshal(value: &MarshalValue) -> VmResult<Self> {
        match value {
            MarshalValue::None => Ok(Self::None),
            MarshalValue::Bool(value) => Ok(Self::Bool(*value)),
            MarshalValue::StopIteration => Err(VmError::UnsupportedConstant {
                description: "StopIteration".to_string(),
            }),
            MarshalValue::Ellipsis => Err(VmError::UnsupportedConstant {
                description: "Ellipsis".to_string(),
            }),
            MarshalValue::Int(value) => Ok(Self::Int(*value)),
            MarshalValue::Long(value) => Ok(Self::Int(long_to_i64(value)?)),
            MarshalValue::Float(value) => Ok(Self::Float(*value)),
            MarshalValue::Complex { .. } => Err(VmError::UnsupportedConstant {
                description: "complex".to_string(),
            }),
            MarshalValue::Bytes(value) => Ok(Self::Bytes(value.clone())),
            MarshalValue::String(value) => Ok(Self::String(value.clone())),
            MarshalValue::Tuple(values) => Ok(Self::Tuple(
                values
                    .iter()
                    .map(Self::from_marshal)
                    .collect::<VmResult<Vec<_>>>()?,
            )),
            MarshalValue::List(values) => Ok(Self::List(
                values
                    .iter()
                    .map(Self::from_marshal)
                    .collect::<VmResult<Vec<_>>>()?,
            )),
            MarshalValue::Set(values) => Ok(Self::Set(
                values
                    .iter()
                    .map(Self::from_marshal)
                    .collect::<VmResult<Vec<_>>>()?,
            )),
            MarshalValue::FrozenSet(values) => Ok(Self::FrozenSet(
                values
                    .iter()
                    .map(Self::from_marshal)
                    .collect::<VmResult<Vec<_>>>()?,
            )),
            MarshalValue::Dict(entries) => Ok(Self::Dict(
                entries
                    .iter()
                    .map(|(key, value)| Ok((Self::from_marshal(key)?, Self::from_marshal(value)?)))
                    .collect::<VmResult<Vec<_>>>()?,
            )),
            MarshalValue::Code(code) => Ok(Self::Code(Box::new((**code).clone()))),
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::None => "NoneType",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::String(_) => "str",
            Self::Bytes(_) => "bytes",
            Self::Tuple(_) => "tuple",
            Self::List(_) => "list",
            Self::Set(_) => "set",
            Self::FrozenSet(_) => "frozenset",
            Self::Dict(_) => "dict",
            Self::Code(_) => "code",
            Self::BuiltinPrint => "builtin_function_or_method",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Self::None => false,
            Self::Bool(value) => *value,
            Self::Int(value) => *value != 0,
            Self::Float(value) => *value != 0.0,
            Self::String(value) => !value.is_empty(),
            Self::Bytes(value) => !value.is_empty(),
            Self::Tuple(value) | Self::List(value) | Self::Set(value) | Self::FrozenSet(value) => {
                !value.is_empty()
            }
            Self::Dict(value) => !value.is_empty(),
            Self::Code(_) => true,
            Self::BuiltinPrint => true,
        }
    }

    pub fn as_index(&self) -> Option<i64> {
        match self {
            Self::Int(value) => Some(*value),
            Self::Bool(value) => Some(i64::from(*value)),
            _ => None,
        }
    }

    pub fn iterable(&self) -> Option<&[VmValue]> {
        match self {
            Self::Tuple(values)
            | Self::List(values)
            | Self::Set(values)
            | Self::FrozenSet(values) => Some(values),
            _ => None,
        }
    }
}

impl std::fmt::Display for VmValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Bool(value) => write!(f, "{}", if *value { "True" } else { "False" }),
            Self::Int(value) => write!(f, "{value}"),
            Self::Float(value) => write!(f, "{value}"),
            Self::String(value) => write!(f, "{value:?}"),
            Self::Bytes(value) => write!(f, "bytes({value:?})"),
            Self::Tuple(values) => write_sequence(f, "(", ")", values),
            Self::List(values) => write_sequence(f, "[", "]", values),
            Self::Set(values) => write_sequence(f, "{", "}", values),
            Self::FrozenSet(values) => write_sequence(f, "frozenset({", "})", values),
            Self::Dict(entries) => {
                write!(f, "{{")?;
                for (index, (key, value)) in entries.iter().enumerate() {
                    if index != 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{key}: {value}")?;
                }
                write!(f, "}}")
            }
            Self::Code(code) => write!(f, "<code object {}>", code.name),
            Self::BuiltinPrint => write!(f, "<built-in function print>"),
        }
    }
}

impl VmValue {
    pub(crate) fn print_text(&self) -> String {
        match self {
            Self::String(value) => value.clone(),
            Self::Bytes(value) => format!("b{value:?}"),
            value => value.to_string(),
        }
    }
}

fn write_sequence(
    f: &mut std::fmt::Formatter<'_>,
    open: &str,
    close: &str,
    values: &[VmValue],
) -> std::fmt::Result {
    write!(f, "{open}")?;
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            write!(f, ", ")?;
        }
        write!(f, "{value}")?;
    }
    write!(f, "{close}")
}

fn long_to_i64(value: &MarshalLong) -> VmResult<i64> {
    let mut magnitude = 0u128;
    for digit in value.digits.iter().rev() {
        magnitude = magnitude
            .checked_mul(1 << 15)
            .and_then(|value| value.checked_add(u128::from(*digit)))
            .ok_or_else(|| VmError::UnsupportedConstant {
                description: "integer larger than int64".to_string(),
            })?;
    }

    if value.negative {
        if magnitude > (i64::MAX as u128) + 1 {
            return Err(VmError::UnsupportedConstant {
                description: "integer smaller than int64".to_string(),
            });
        }
        if magnitude == (i64::MAX as u128) + 1 {
            Ok(i64::MIN)
        } else {
            Ok(-(magnitude as i64))
        }
    } else {
        i64::try_from(magnitude).map_err(|_| VmError::UnsupportedConstant {
            description: "integer larger than int64".to_string(),
        })
    }
}
