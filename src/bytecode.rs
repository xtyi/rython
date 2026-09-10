use crate::code::CodeObject;
use crate::error::{BytecodeError, BytecodeResult};

pub const WORD_SIZE: usize = 2;

const CACHE: u8 = 0;
const POP_TOP: u8 = 1;
const PUSH_NULL: u8 = 2;
const NOP: u8 = 9;
const UNARY_POSITIVE: u8 = 10;
const UNARY_NEGATIVE: u8 = 11;
const UNARY_NOT: u8 = 12;
const UNARY_INVERT: u8 = 15;
const BINARY_SUBSCR: u8 = 25;
const STORE_SUBSCR: u8 = 60;
const FOR_ITER: u8 = 93;
const RETURN_VALUE: u8 = 83;
const STORE_NAME: u8 = 90;
const SWAP: u8 = 99;
const LOAD_CONST: u8 = 100;
const LOAD_NAME: u8 = 101;
const BUILD_TUPLE: u8 = 102;
const BUILD_LIST: u8 = 103;
const BUILD_SET: u8 = 104;
const BUILD_MAP: u8 = 105;
const COMPARE_OP: u8 = 107;
const JUMP_FORWARD: u8 = 110;
const POP_JUMP_IF_FALSE: u8 = 114;
const POP_JUMP_IF_TRUE: u8 = 115;
const LOAD_GLOBAL: u8 = 116;
const COPY: u8 = 120;
const RETURN_CONST: u8 = 121;
const BINARY_OP: u8 = 122;
const LOAD_FAST: u8 = 124;
const STORE_FAST: u8 = 125;
const JUMP_BACKWARD_NO_INTERRUPT: u8 = 134;
const JUMP_BACKWARD: u8 = 140;
const EXTENDED_ARG: u8 = 144;
const RESUME: u8 = 151;
const BUILD_CONST_KEY_MAP: u8 = 156;
const LIST_EXTEND: u8 = 162;
const SET_UPDATE: u8 = 163;
const CALL: u8 = 171;
const CALL_INTRINSIC_1: u8 = 173;
const MAKE_FUNCTION: u8 = 132;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    Cache,
    PopTop,
    PushNull,
    Nop,
    UnaryPositive,
    UnaryNegative,
    UnaryNot,
    UnaryInvert,
    BinarySubscr,
    StoreSubscr,
    ForIter,
    ReturnValue,
    StoreName,
    Swap,
    LoadConst,
    LoadName,
    BuildTuple,
    BuildList,
    BuildSet,
    BuildMap,
    CompareOp,
    JumpForward,
    PopJumpIfFalse,
    PopJumpIfTrue,
    LoadGlobal,
    Copy,
    ReturnConst,
    BinaryOp,
    LoadFast,
    StoreFast,
    JumpBackwardNoInterrupt,
    JumpBackward,
    ExtendedArg,
    Resume,
    BuildConstKeyMap,
    ListExtend,
    SetUpdate,
    Call,
    CallIntrinsic1,
    MakeFunction,
    Unknown(u8),
}

impl Opcode {
    pub fn from_byte(opcode: u8) -> Self {
        match opcode {
            CACHE => Self::Cache,
            POP_TOP => Self::PopTop,
            PUSH_NULL => Self::PushNull,
            NOP => Self::Nop,
            UNARY_POSITIVE => Self::UnaryPositive,
            UNARY_NEGATIVE => Self::UnaryNegative,
            UNARY_NOT => Self::UnaryNot,
            UNARY_INVERT => Self::UnaryInvert,
            BINARY_SUBSCR => Self::BinarySubscr,
            STORE_SUBSCR => Self::StoreSubscr,
            FOR_ITER => Self::ForIter,
            RETURN_VALUE => Self::ReturnValue,
            STORE_NAME => Self::StoreName,
            SWAP => Self::Swap,
            LOAD_CONST => Self::LoadConst,
            LOAD_NAME => Self::LoadName,
            BUILD_TUPLE => Self::BuildTuple,
            BUILD_LIST => Self::BuildList,
            BUILD_SET => Self::BuildSet,
            BUILD_MAP => Self::BuildMap,
            COMPARE_OP => Self::CompareOp,
            JUMP_FORWARD => Self::JumpForward,
            POP_JUMP_IF_FALSE => Self::PopJumpIfFalse,
            POP_JUMP_IF_TRUE => Self::PopJumpIfTrue,
            LOAD_GLOBAL => Self::LoadGlobal,
            COPY => Self::Copy,
            RETURN_CONST => Self::ReturnConst,
            BINARY_OP => Self::BinaryOp,
            LOAD_FAST => Self::LoadFast,
            STORE_FAST => Self::StoreFast,
            JUMP_BACKWARD_NO_INTERRUPT => Self::JumpBackwardNoInterrupt,
            JUMP_BACKWARD => Self::JumpBackward,
            EXTENDED_ARG => Self::ExtendedArg,
            RESUME => Self::Resume,
            BUILD_CONST_KEY_MAP => Self::BuildConstKeyMap,
            LIST_EXTEND => Self::ListExtend,
            SET_UPDATE => Self::SetUpdate,
            CALL => Self::Call,
            CALL_INTRINSIC_1 => Self::CallIntrinsic1,
            MAKE_FUNCTION => Self::MakeFunction,
            _ => Self::Unknown(opcode),
        }
    }

    pub const fn raw(self) -> u8 {
        match self {
            Self::Cache => CACHE,
            Self::PopTop => POP_TOP,
            Self::PushNull => PUSH_NULL,
            Self::Nop => NOP,
            Self::UnaryPositive => UNARY_POSITIVE,
            Self::UnaryNegative => UNARY_NEGATIVE,
            Self::UnaryNot => UNARY_NOT,
            Self::UnaryInvert => UNARY_INVERT,
            Self::BinarySubscr => BINARY_SUBSCR,
            Self::StoreSubscr => STORE_SUBSCR,
            Self::ForIter => FOR_ITER,
            Self::ReturnValue => RETURN_VALUE,
            Self::StoreName => STORE_NAME,
            Self::Swap => SWAP,
            Self::LoadConst => LOAD_CONST,
            Self::LoadName => LOAD_NAME,
            Self::BuildTuple => BUILD_TUPLE,
            Self::BuildList => BUILD_LIST,
            Self::BuildSet => BUILD_SET,
            Self::BuildMap => BUILD_MAP,
            Self::CompareOp => COMPARE_OP,
            Self::JumpForward => JUMP_FORWARD,
            Self::PopJumpIfFalse => POP_JUMP_IF_FALSE,
            Self::PopJumpIfTrue => POP_JUMP_IF_TRUE,
            Self::LoadGlobal => LOAD_GLOBAL,
            Self::Copy => COPY,
            Self::ReturnConst => RETURN_CONST,
            Self::BinaryOp => BINARY_OP,
            Self::LoadFast => LOAD_FAST,
            Self::StoreFast => STORE_FAST,
            Self::JumpBackwardNoInterrupt => JUMP_BACKWARD_NO_INTERRUPT,
            Self::JumpBackward => JUMP_BACKWARD,
            Self::ExtendedArg => EXTENDED_ARG,
            Self::Resume => RESUME,
            Self::BuildConstKeyMap => BUILD_CONST_KEY_MAP,
            Self::ListExtend => LIST_EXTEND,
            Self::SetUpdate => SET_UPDATE,
            Self::Call => CALL,
            Self::CallIntrinsic1 => CALL_INTRINSIC_1,
            Self::MakeFunction => MAKE_FUNCTION,
            Self::Unknown(opcode) => opcode,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instruction {
    pub offset: usize,
    pub opcode: Opcode,
    pub arg: u32,
    pub size: usize,
}

impl Instruction {
    pub const fn raw_opcode(self) -> u8 {
        self.opcode.raw()
    }
}

pub fn decode_code(code: &CodeObject) -> BytecodeResult<Vec<Instruction>> {
    decode(&code.bytecode)
}

pub fn decode(bytecode: &[u8]) -> BytecodeResult<Vec<Instruction>> {
    if !bytecode.len().is_multiple_of(WORD_SIZE) {
        return Err(BytecodeError::OddLength {
            length: bytecode.len(),
        });
    }

    let mut instructions = Vec::new();
    let mut offset = 0;
    let mut extended_arg = 0u32;
    let mut extended_offset = None;

    while offset < bytecode.len() {
        let instruction_offset = offset;
        let opcode = bytecode[offset];
        let argument = bytecode[offset + 1] as u32;
        offset += WORD_SIZE;

        if opcode == EXTENDED_ARG {
            extended_arg = extended_arg
                .checked_shl(8)
                .and_then(|value| value.checked_add(argument))
                .ok_or(BytecodeError::ExtendedArgOverflow {
                    offset: instruction_offset,
                })?;
            extended_offset.get_or_insert(instruction_offset);
            continue;
        }

        let full_argument = extended_arg
            .checked_shl(8)
            .and_then(|value| value.checked_add(argument))
            .ok_or(BytecodeError::ExtendedArgOverflow {
                offset: instruction_offset,
            })?;
        let start = extended_offset.unwrap_or(instruction_offset);
        instructions.push(Instruction {
            offset: start,
            opcode: Opcode::from_byte(opcode),
            arg: full_argument,
            size: offset - start,
        });
        extended_arg = 0;
        extended_offset = None;
    }

    if let Some(offset) = extended_offset {
        return Err(BytecodeError::DanglingExtendedArg { offset });
    }

    Ok(instructions)
}
