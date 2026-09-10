use std::collections::{BTreeMap, HashMap};

use crate::bytecode::{decode_code, Instruction, Opcode};
use crate::code::CodeObject;
use crate::error::{VmError, VmResult};
use crate::pyc::PycFile;
use crate::value::VmValue;

const BINARY_OP_INPLACE_OFFSET: u32 = 13;

#[derive(Debug)]
pub struct Vm {
    globals: BTreeMap<String, VmValue>,
    output: String,
}

impl Default for Vm {
    fn default() -> Self {
        let mut globals = BTreeMap::new();
        globals.insert("print".to_string(), VmValue::BuiltinPrint);
        Self {
            globals,
            output: String::new(),
        }
    }
}

impl Vm {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn globals(&self) -> &BTreeMap<String, VmValue> {
        &self.globals
    }

    pub fn global(&self, name: &str) -> Option<&VmValue> {
        self.globals.get(name)
    }

    pub fn output(&self) -> &str {
        &self.output
    }

    pub fn take_output(&mut self) -> String {
        std::mem::take(&mut self.output)
    }

    pub fn run_pyc(&mut self, pyc: &PycFile) -> VmResult<VmValue> {
        self.run_code(&pyc.code)
    }

    pub fn run_code(&mut self, code: &CodeObject) -> VmResult<VmValue> {
        self.run_code_with_locals(code, Vec::new())
    }

    pub fn run_code_with_locals(
        &mut self,
        code: &CodeObject,
        locals: Vec<VmValue>,
    ) -> VmResult<VmValue> {
        let instructions = decode_code(code)?;
        let offsets = instructions
            .iter()
            .enumerate()
            .map(|(index, instruction)| (instruction.offset, index))
            .collect::<HashMap<_, _>>();
        let mut frame = Frame::new(code, instructions, offsets, locals)?;
        let mut ip = 0usize;

        loop {
            let instruction =
                frame
                    .instructions
                    .get(ip)
                    .copied()
                    .ok_or_else(|| VmError::MissingReturn {
                        code_name: code.name.clone(),
                    })?;
            let mut next_ip = ip + 1;

            match instruction.opcode {
                Opcode::Cache
                | Opcode::Nop
                | Opcode::Resume
                | Opcode::ExtendedArg
                | Opcode::PushNull => {
                    if instruction.opcode == Opcode::PushNull {
                        frame.stack.push(VmValue::None);
                    }
                }
                Opcode::PopTop => {
                    frame.pop(instruction)?;
                }
                Opcode::LoadConst => {
                    let value = Self::load_constant(code, instruction)?;
                    frame.stack.push(value);
                }
                Opcode::StoreName => {
                    let name = Self::name(code, instruction)?;
                    let value = frame.pop(instruction)?;
                    self.globals.insert(name.to_string(), value);
                }
                Opcode::LoadName => {
                    let name = Self::name(code, instruction)?;
                    let value =
                        self.globals
                            .get(name)
                            .cloned()
                            .ok_or_else(|| VmError::UnknownName {
                                offset: instruction.offset,
                                name: name.to_string(),
                            })?;
                    frame.stack.push(value);
                }
                Opcode::LoadGlobal => {
                    let name_index = instruction.arg >> 1;
                    let name = Self::name_by_index(code, instruction, name_index)?;
                    let value =
                        self.globals
                            .get(name)
                            .cloned()
                            .ok_or_else(|| VmError::UnknownName {
                                offset: instruction.offset,
                                name: name.to_string(),
                            })?;
                    frame.stack.push(value);
                }
                Opcode::StoreFast => {
                    let name = Self::local_name(code, instruction)?;
                    let value = frame.pop(instruction)?;
                    frame.locals[instruction.arg as usize] = Some(value);
                    let _ = name;
                }
                Opcode::LoadFast => {
                    let name = Self::local_name(code, instruction)?;
                    let value =
                        frame.locals[instruction.arg as usize]
                            .clone()
                            .ok_or_else(|| VmError::UnboundLocal {
                                offset: instruction.offset,
                                name: name.to_string(),
                            })?;
                    frame.stack.push(value);
                }
                Opcode::UnaryNegative => {
                    let value = frame.pop(instruction)?;
                    frame.stack.push(Self::unary_negative(instruction, value)?);
                }
                Opcode::UnaryPositive => {
                    let value = frame.pop(instruction)?;
                    match value {
                        VmValue::Int(_) | VmValue::Float(_) | VmValue::Bool(_) => {
                            frame.stack.push(match value {
                                VmValue::Bool(value) => VmValue::Int(i64::from(value)),
                                value => value,
                            });
                        }
                        other => {
                            return Err(Self::type_error(instruction, "unary +", &other, None))
                        }
                    }
                }
                Opcode::UnaryNot => {
                    let value = frame.pop(instruction)?;
                    frame.stack.push(VmValue::Bool(!value.is_truthy()));
                }
                Opcode::UnaryInvert => {
                    let value = frame.pop(instruction)?;
                    let value = match value {
                        VmValue::Int(value) => VmValue::Int(!value),
                        VmValue::Bool(value) => VmValue::Int(!(value as i64)),
                        other => {
                            return Err(Self::type_error(instruction, "unary ~", &other, None))
                        }
                    };
                    frame.stack.push(value);
                }
                Opcode::BinaryOp => {
                    let right = frame.pop(instruction)?;
                    let left = frame.pop(instruction)?;
                    let value = Self::binary_op(instruction, left, right)?;
                    frame.stack.push(value);
                }
                Opcode::CompareOp => {
                    let right = frame.pop(instruction)?;
                    let left = frame.pop(instruction)?;
                    let value = Self::compare_op(instruction, left, right)?;
                    frame.stack.push(VmValue::Bool(value));
                }
                Opcode::BuildTuple => {
                    let values = frame.pop_many(instruction, instruction.arg as usize)?;
                    frame.stack.push(VmValue::Tuple(values));
                }
                Opcode::BuildList => {
                    let values = frame.pop_many(instruction, instruction.arg as usize)?;
                    frame.stack.push(VmValue::List(values));
                }
                Opcode::BuildSet => {
                    let values = frame.pop_many(instruction, instruction.arg as usize)?;
                    frame.stack.push(VmValue::Set(unique(values)));
                }
                Opcode::BuildMap => {
                    let pair_count = instruction.arg as usize;
                    let mut entries = Vec::with_capacity(pair_count);
                    for _ in 0..pair_count {
                        let value = frame.pop(instruction)?;
                        let key = frame.pop(instruction)?;
                        entries.push((key, value));
                    }
                    entries.reverse();
                    frame.stack.push(VmValue::Dict(entries));
                }
                Opcode::BuildConstKeyMap => {
                    let key_count = instruction.arg as usize;
                    let keys = match frame.pop(instruction)? {
                        VmValue::Tuple(keys) if keys.len() == key_count => keys,
                        VmValue::Tuple(keys) => {
                            return Err(Self::invalid_argument(
                                instruction,
                                format!("expected {key_count} dictionary keys, got {}", keys.len()),
                            ))
                        }
                        other => {
                            return Err(Self::type_error(
                                instruction,
                                "BUILD_CONST_KEY_MAP",
                                &other,
                                None,
                            ))
                        }
                    };
                    let values = frame.pop_many(instruction, key_count)?;
                    frame
                        .stack
                        .push(VmValue::Dict(keys.into_iter().zip(values).collect()));
                }
                Opcode::ListExtend => {
                    Self::extend_sequence(&mut frame, instruction, false)?;
                }
                Opcode::SetUpdate => {
                    Self::extend_sequence(&mut frame, instruction, true)?;
                }
                Opcode::BinarySubscr => {
                    let index = frame.pop(instruction)?;
                    let container = frame.pop(instruction)?;
                    frame
                        .stack
                        .push(Self::load_subscript(instruction, container, index)?);
                }
                Opcode::StoreSubscr => {
                    let index = frame.pop(instruction)?;
                    let mut container = frame.pop(instruction)?;
                    let value = frame.pop(instruction)?;
                    let original = container.clone();
                    Self::store_subscript(instruction, &mut container, index, value)?;
                    self.replace_global_container(&original, container);
                }
                Opcode::JumpForward => {
                    next_ip = Self::jump_target(&frame, instruction, true)?;
                }
                Opcode::JumpBackward | Opcode::JumpBackwardNoInterrupt => {
                    next_ip = Self::jump_target(&frame, instruction, false)?;
                }
                Opcode::PopJumpIfFalse => {
                    let condition = frame.pop(instruction)?;
                    if !condition.is_truthy() {
                        next_ip = Self::jump_target(&frame, instruction, true)?;
                    }
                }
                Opcode::PopJumpIfTrue => {
                    let condition = frame.pop(instruction)?;
                    if condition.is_truthy() {
                        next_ip = Self::jump_target(&frame, instruction, true)?;
                    }
                }
                Opcode::ReturnConst => {
                    return Self::load_constant(code, instruction);
                }
                Opcode::ReturnValue => {
                    return frame.pop(instruction);
                }
                Opcode::Copy => {
                    let index = Self::stack_index(&frame, instruction)?;
                    let value = frame.stack[index].clone();
                    frame.stack.push(value);
                }
                Opcode::Swap => {
                    let index = Self::stack_index(&frame, instruction)?;
                    let top = frame.stack.len() - 1;
                    frame.stack.swap(index, top);
                }
                Opcode::CallIntrinsic1 => {
                    if instruction.arg != 5 {
                        return Err(Self::invalid_argument(
                            instruction,
                            "only INTRINSIC_UNARY_POSITIVE is supported".to_string(),
                        ));
                    }
                    let value = frame.pop(instruction)?;
                    match value {
                        VmValue::Int(_) | VmValue::Float(_) | VmValue::Bool(_) => {
                            frame.stack.push(match value {
                                VmValue::Bool(value) => VmValue::Int(i64::from(value)),
                                value => value,
                            });
                        }
                        other => {
                            return Err(Self::type_error(instruction, "unary +", &other, None))
                        }
                    }
                }
                Opcode::Call => {
                    let arguments = frame.pop_many(instruction, instruction.arg as usize)?;
                    let callable = frame.pop(instruction)?;
                    if matches!(frame.stack.last(), Some(VmValue::None)) {
                        frame.stack.pop();
                    }
                    let result = match callable {
                        VmValue::BuiltinPrint => self.call_print(arguments),
                        other => return Err(Self::type_error(instruction, "call", &other, None)),
                    };
                    frame.stack.push(result);
                }
                Opcode::ForIter | Opcode::MakeFunction => {
                    return Err(VmError::UnsupportedOpcode {
                        offset: instruction.offset,
                        opcode: instruction.raw_opcode(),
                    });
                }
                Opcode::Unknown(opcode) => {
                    return Err(VmError::UnsupportedOpcode {
                        offset: instruction.offset,
                        opcode,
                    });
                }
            }

            ip = next_ip;
        }
    }

    fn load_constant(code: &CodeObject, instruction: Instruction) -> VmResult<VmValue> {
        let value = code.constants.get(instruction.arg as usize).ok_or(
            VmError::ConstantIndexOutOfBounds {
                offset: instruction.offset,
                index: instruction.arg,
            },
        )?;
        VmValue::from_marshal(value)
    }

    fn name(code: &CodeObject, instruction: Instruction) -> VmResult<&str> {
        Self::name_by_index(code, instruction, instruction.arg)
    }

    fn name_by_index(code: &CodeObject, instruction: Instruction, index: u32) -> VmResult<&str> {
        code.names
            .get(index as usize)
            .map(String::as_str)
            .ok_or(VmError::NameIndexOutOfBounds {
                offset: instruction.offset,
                index,
            })
    }

    fn local_name(code: &CodeObject, instruction: Instruction) -> VmResult<&str> {
        code.localsplus_names
            .get(instruction.arg as usize)
            .map(String::as_str)
            .ok_or(VmError::LocalIndexOutOfBounds {
                offset: instruction.offset,
                index: instruction.arg,
            })
    }

    fn unary_negative(instruction: Instruction, value: VmValue) -> VmResult<VmValue> {
        match value {
            VmValue::Int(value) => {
                value
                    .checked_neg()
                    .map(VmValue::Int)
                    .ok_or_else(|| VmError::IntegerOverflow {
                        offset: instruction.offset,
                        operation: "unary -".to_string(),
                    })
            }
            VmValue::Float(value) => Ok(VmValue::Float(-value)),
            VmValue::Bool(value) => Ok(VmValue::Int(-(value as i64))),
            other => Err(Self::type_error(instruction, "unary -", &other, None)),
        }
    }

    fn binary_op(instruction: Instruction, left: VmValue, right: VmValue) -> VmResult<VmValue> {
        let operation = instruction.arg % BINARY_OP_INPLACE_OFFSET;

        if operation == 0 {
            match (&left, &right) {
                (VmValue::String(left), VmValue::String(right)) => {
                    return Ok(VmValue::String(format!("{left}{right}")));
                }
                (VmValue::Bytes(left), VmValue::Bytes(right)) => {
                    let mut value = left.clone();
                    value.extend_from_slice(right);
                    return Ok(VmValue::Bytes(value));
                }
                (VmValue::List(left), VmValue::List(right)) => {
                    let mut value = left.clone();
                    value.extend(right.clone());
                    return Ok(VmValue::List(value));
                }
                (VmValue::Tuple(left), VmValue::Tuple(right)) => {
                    let mut value = left.clone();
                    value.extend(right.clone());
                    return Ok(VmValue::Tuple(value));
                }
                _ => {}
            }
        }

        if operation == 5 {
            if let Some(value) = repeat_sequence(&left, &right) {
                return value;
            }
            if let Some(value) = repeat_sequence(&right, &left) {
                return value;
            }
        }

        let left_numeric = numeric(&left);
        let right_numeric = numeric(&right);
        if let (Some(left), Some(right)) = (left_numeric, right_numeric) {
            return Self::numeric_binary(instruction, operation, left, right);
        }

        Err(Self::type_error(
            instruction,
            binary_operation_name(operation),
            &left,
            Some(&right),
        ))
    }

    fn numeric_binary(
        instruction: Instruction,
        operation: u32,
        left: Numeric,
        right: Numeric,
    ) -> VmResult<VmValue> {
        if operation == 1 || operation == 3 || operation == 7 || operation == 9 || operation == 12 {
            let (left, right) = match (left, right) {
                (Numeric::Int(left), Numeric::Int(right)) => (left, right),
                _ => {
                    return Err(VmError::TypeError {
                        offset: instruction.offset,
                        operation: binary_operation_name(operation).to_string(),
                        left: "float".to_string(),
                        right: Some("float".to_string()),
                    })
                }
            };
            let value = match operation {
                1 => left & right,
                3 => checked_shift(instruction, left, right, true)?,
                7 => left | right,
                9 => checked_shift(instruction, left, right, false)?,
                12 => left ^ right,
                _ => unreachable!(),
            };
            return Ok(VmValue::Int(value));
        }

        if (operation == 2 || operation == 6) && right.as_f64() == 0.0 {
            return Err(VmError::ZeroDivision {
                offset: instruction.offset,
            });
        }

        if operation == 11 {
            if right.as_f64() == 0.0 {
                return Err(VmError::ZeroDivision {
                    offset: instruction.offset,
                });
            }
            return Ok(VmValue::Float(left.as_f64() / right.as_f64()));
        }

        if operation == 8 {
            if let (Numeric::Int(left), Numeric::Int(right)) = (left, right) {
                if right >= 0 {
                    let exponent = u32::try_from(right).map_err(|_| VmError::IntegerOverflow {
                        offset: instruction.offset,
                        operation: "**".to_string(),
                    })?;
                    if let Some(value) = left.checked_pow(exponent) {
                        return Ok(VmValue::Int(value));
                    }
                }
            }
            return Ok(VmValue::Float(left.as_f64().powf(right.as_f64())));
        }

        if matches!(left, Numeric::Float(_)) || matches!(right, Numeric::Float(_)) {
            let left = left.as_f64();
            let right = right.as_f64();
            let value = match operation {
                0 => left + right,
                2 => (left / right).floor(),
                5 => left * right,
                6 => left - (left / right).floor() * right,
                10 => left - right,
                _ => {
                    return Err(VmError::UnsupportedOperation {
                        offset: instruction.offset,
                        operation: binary_operation_name(operation).to_string(),
                    })
                }
            };
            return Ok(VmValue::Float(value));
        }

        let (left, right) = match (left, right) {
            (Numeric::Int(left), Numeric::Int(right)) => (left, right),
            _ => unreachable!(),
        };
        let value = match operation {
            0 => left.checked_add(right),
            2 => Some(floor_div_int(left, right, instruction)?),
            5 => left.checked_mul(right),
            6 => Some(remainder_int(left, right, instruction)?),
            10 => left.checked_sub(right),
            _ => {
                return Err(VmError::UnsupportedOperation {
                    offset: instruction.offset,
                    operation: binary_operation_name(operation).to_string(),
                })
            }
        };
        value
            .map(VmValue::Int)
            .ok_or_else(|| VmError::IntegerOverflow {
                offset: instruction.offset,
                operation: binary_operation_name(operation).to_string(),
            })
    }

    fn compare_op(instruction: Instruction, left: VmValue, right: VmValue) -> VmResult<bool> {
        let compare = instruction.arg >> 4;
        if compare > 5 {
            return Err(Self::invalid_argument(
                instruction,
                format!("unknown COMPARE_OP selector {}", instruction.arg),
            ));
        }

        if compare == 2 || compare == 3 {
            let equal = python_equal(&left, &right);
            return Ok(if compare == 2 { equal } else { !equal });
        }

        let ordering = match (numeric(&left), numeric(&right)) {
            (Some(left), Some(right)) => {
                left.as_f64()
                    .partial_cmp(&right.as_f64())
                    .ok_or_else(|| VmError::ValueError {
                        offset: instruction.offset,
                        operation: "comparison with NaN".to_string(),
                    })?
            }
            _ => match (&left, &right) {
                (VmValue::String(left), VmValue::String(right)) => left.cmp(right),
                (VmValue::Bytes(left), VmValue::Bytes(right)) => left.cmp(right),
                _ => {
                    return Err(Self::type_error(
                        instruction,
                        "comparison",
                        &left,
                        Some(&right),
                    ))
                }
            },
        };

        Ok(match compare {
            0 => ordering.is_lt(),
            1 => ordering.is_le(),
            4 => ordering.is_gt(),
            5 => ordering.is_ge(),
            _ => unreachable!(),
        })
    }

    fn load_subscript(
        instruction: Instruction,
        container: VmValue,
        index: VmValue,
    ) -> VmResult<VmValue> {
        match container {
            VmValue::List(values) | VmValue::Tuple(values) => {
                let index = Self::sequence_index(instruction, values.len(), &index)?;
                Ok(values[index].clone())
            }
            VmValue::String(value) => {
                let index = Self::sequence_index(instruction, value.chars().count(), &index)?;
                Ok(VmValue::String(
                    value
                        .chars()
                        .nth(index)
                        .expect("validated character index")
                        .to_string(),
                ))
            }
            VmValue::Bytes(value) => {
                let index = Self::sequence_index(instruction, value.len(), &index)?;
                Ok(VmValue::Int(i64::from(value[index])))
            }
            VmValue::Dict(entries) => entries
                .into_iter()
                .find(|(key, _)| python_equal(key, &index))
                .map(|(_, value)| value)
                .ok_or(VmError::KeyError {
                    offset: instruction.offset,
                }),
            other => Err(Self::type_error(
                instruction,
                "subscription",
                &other,
                Some(&index),
            )),
        }
    }

    fn store_subscript(
        instruction: Instruction,
        container: &mut VmValue,
        index: VmValue,
        value: VmValue,
    ) -> VmResult<()> {
        match container {
            VmValue::List(values) => {
                let index = Self::sequence_index(instruction, values.len(), &index)?;
                values[index] = value;
                Ok(())
            }
            VmValue::Dict(entries) => {
                if let Some((_, existing)) = entries
                    .iter_mut()
                    .find(|(key, _)| python_equal(key, &index))
                {
                    *existing = value;
                } else {
                    entries.push((index, value));
                }
                Ok(())
            }
            other => Err(Self::type_error(
                instruction,
                "item assignment",
                other,
                Some(&index),
            )),
        }
    }

    fn sequence_index(instruction: Instruction, length: usize, index: &VmValue) -> VmResult<usize> {
        let index = index
            .as_index()
            .ok_or_else(|| Self::type_error(instruction, "sequence index", index, None))?;
        let normalized = if index < 0 {
            (length as i64).checked_add(index)
        } else {
            Some(index)
        }
        .ok_or(VmError::IndexError {
            offset: instruction.offset,
            index,
        })?;
        if normalized < 0 || normalized >= length as i64 {
            return Err(VmError::IndexError {
                offset: instruction.offset,
                index,
            });
        }
        Ok(normalized as usize)
    }

    fn extend_sequence(
        frame: &mut Frame<'_>,
        instruction: Instruction,
        is_set: bool,
    ) -> VmResult<()> {
        let iterable = frame.pop(instruction)?;
        let values = iterable
            .iterable()
            .ok_or_else(|| Self::type_error(instruction, "iteration", &iterable, None))?
            .to_vec();
        let position = frame
            .stack
            .len()
            .checked_sub(instruction.arg as usize)
            .ok_or(VmError::StackUnderflow {
                offset: instruction.offset,
                opcode: instruction.raw_opcode(),
            })?;
        let target = frame
            .stack
            .get_mut(position)
            .ok_or(VmError::StackUnderflow {
                offset: instruction.offset,
                opcode: instruction.raw_opcode(),
            })?;
        match target {
            VmValue::List(target) if !is_set => target.extend(values),
            VmValue::Set(target) if is_set => target.extend(unique(values)),
            other => {
                return Err(Self::type_error(
                    instruction,
                    if is_set { "set update" } else { "list extend" },
                    other,
                    None,
                ))
            }
        }
        Ok(())
    }

    fn stack_index(frame: &Frame<'_>, instruction: Instruction) -> VmResult<usize> {
        if instruction.arg == 0 || instruction.arg as usize > frame.stack.len() {
            return Err(Self::invalid_argument(
                instruction,
                "stack position is out of bounds".to_string(),
            ));
        }
        Ok(frame.stack.len() - instruction.arg as usize)
    }

    fn jump_target(frame: &Frame<'_>, instruction: Instruction, forward: bool) -> VmResult<usize> {
        let distance = (instruction.arg as usize)
            .checked_mul(2)
            .ok_or_else(|| Self::invalid_argument(instruction, "jump distance overflow".into()))?;
        let base = instruction
            .offset
            .checked_add(instruction.size)
            .ok_or_else(|| Self::invalid_argument(instruction, "jump base overflow".into()))?;
        let target = if forward {
            base.checked_add(distance)
        } else {
            base.checked_sub(distance)
        }
        .ok_or(VmError::InvalidJumpTarget {
            offset: instruction.offset,
            target: usize::MAX,
        })?;
        frame
            .offsets
            .get(&target)
            .copied()
            .ok_or(VmError::InvalidJumpTarget {
                offset: instruction.offset,
                target,
            })
    }

    fn type_error(
        instruction: Instruction,
        operation: &str,
        left: &VmValue,
        right: Option<&VmValue>,
    ) -> VmError {
        VmError::TypeError {
            offset: instruction.offset,
            operation: operation.to_string(),
            left: left.type_name().to_string(),
            right: right.map(|value| value.type_name().to_string()),
        }
    }

    fn invalid_argument(instruction: Instruction, reason: String) -> VmError {
        VmError::InvalidArgument {
            offset: instruction.offset,
            opcode: instruction.raw_opcode(),
            argument: instruction.arg,
            reason,
        }
    }

    fn call_print(&mut self, arguments: Vec<VmValue>) -> VmValue {
        for (index, argument) in arguments.iter().enumerate() {
            if index != 0 {
                self.output.push(' ');
            }
            self.output.push_str(&argument.print_text());
        }
        self.output.push('\n');
        VmValue::None
    }

    fn replace_global_container(&mut self, original: &VmValue, updated: VmValue) {
        for value in self.globals.values_mut() {
            if value == original {
                *value = updated.clone();
            }
        }
    }
}

struct Frame<'a> {
    _code: &'a CodeObject,
    instructions: Vec<Instruction>,
    offsets: HashMap<usize, usize>,
    stack: Vec<VmValue>,
    locals: Vec<Option<VmValue>>,
}

impl<'a> Frame<'a> {
    fn new(
        code: &'a CodeObject,
        instructions: Vec<Instruction>,
        offsets: HashMap<usize, usize>,
        locals: Vec<VmValue>,
    ) -> VmResult<Self> {
        let mut local_values = vec![None; code.localsplus_names.len()];
        if locals.len() > local_values.len() {
            return Err(VmError::InvalidArgument {
                offset: 0,
                opcode: 0,
                argument: locals.len() as u32,
                reason: format!(
                    "received {} locals, but code object has {} slots",
                    locals.len(),
                    local_values.len()
                ),
            });
        }
        for (slot, value) in locals.into_iter().enumerate() {
            local_values[slot] = Some(value);
        }
        Ok(Self {
            _code: code,
            instructions,
            offsets,
            stack: Vec::with_capacity(code.stack_size as usize),
            locals: local_values,
        })
    }

    fn pop(&mut self, instruction: Instruction) -> VmResult<VmValue> {
        self.stack.pop().ok_or(VmError::StackUnderflow {
            offset: instruction.offset,
            opcode: instruction.raw_opcode(),
        })
    }

    fn pop_many(&mut self, instruction: Instruction, count: usize) -> VmResult<Vec<VmValue>> {
        if self.stack.len() < count {
            return Err(VmError::StackUnderflow {
                offset: instruction.offset,
                opcode: instruction.raw_opcode(),
            });
        }
        let start = self.stack.len() - count;
        Ok(self.stack.drain(start..).collect())
    }
}

#[derive(Clone, Copy)]
enum Numeric {
    Int(i64),
    Float(f64),
}

impl Numeric {
    fn as_f64(self) -> f64 {
        match self {
            Self::Int(value) => value as f64,
            Self::Float(value) => value,
        }
    }
}

fn numeric(value: &VmValue) -> Option<Numeric> {
    match value {
        VmValue::Bool(value) => Some(Numeric::Int(i64::from(*value))),
        VmValue::Int(value) => Some(Numeric::Int(*value)),
        VmValue::Float(value) => Some(Numeric::Float(*value)),
        _ => None,
    }
}

fn repeat_sequence(left: &VmValue, right: &VmValue) -> Option<VmResult<VmValue>> {
    let count = right.as_index()?;
    let count = if count <= 0 {
        0
    } else {
        usize::try_from(count).ok()?
    };
    match left {
        VmValue::String(value) => Some(Ok(VmValue::String(value.repeat(count)))),
        VmValue::Bytes(value) => Some(Ok(VmValue::Bytes(value.repeat(count)))),
        VmValue::List(value) => Some(Ok(VmValue::List(repeat_values(value, count)))),
        VmValue::Tuple(value) => Some(Ok(VmValue::Tuple(repeat_values(value, count)))),
        _ => None,
    }
}

fn repeat_values(values: &[VmValue], count: usize) -> Vec<VmValue> {
    (0..count).flat_map(|_| values.iter().cloned()).collect()
}

fn unique(values: Vec<VmValue>) -> Vec<VmValue> {
    let mut result = Vec::new();
    for value in values {
        if !result.iter().any(|existing| python_equal(existing, &value)) {
            result.push(value);
        }
    }
    result
}

fn python_equal(left: &VmValue, right: &VmValue) -> bool {
    match (numeric(left), numeric(right)) {
        (Some(left), Some(right)) => left.as_f64() == right.as_f64(),
        _ => left == right,
    }
}

fn checked_shift(
    instruction: Instruction,
    left: i64,
    right: i64,
    left_shift: bool,
) -> VmResult<i64> {
    if !(0..64).contains(&right) {
        return Err(VmError::ValueError {
            offset: instruction.offset,
            operation: "shift count".to_string(),
        });
    }
    let shift = right as u32;
    if left_shift {
        left.checked_shl(shift).ok_or(VmError::IntegerOverflow {
            offset: instruction.offset,
            operation: "<<".to_string(),
        })
    } else {
        Ok(left >> shift)
    }
}

fn floor_div_int(left: i64, right: i64, instruction: Instruction) -> VmResult<i64> {
    if right == 0 {
        return Err(VmError::ZeroDivision {
            offset: instruction.offset,
        });
    }
    let quotient = left / right;
    let remainder = left % right;
    if remainder != 0 && (remainder > 0) != (right > 0) {
        quotient.checked_sub(1).ok_or(VmError::IntegerOverflow {
            offset: instruction.offset,
            operation: "//".to_string(),
        })
    } else {
        Ok(quotient)
    }
}

fn remainder_int(left: i64, right: i64, instruction: Instruction) -> VmResult<i64> {
    let quotient = floor_div_int(left, right, instruction)?;
    left.checked_sub(
        quotient
            .checked_mul(right)
            .ok_or(VmError::IntegerOverflow {
                offset: instruction.offset,
                operation: "%".to_string(),
            })?,
    )
    .ok_or(VmError::IntegerOverflow {
        offset: instruction.offset,
        operation: "%".to_string(),
    })
}

fn binary_operation_name(operation: u32) -> &'static str {
    match operation {
        0 => "+",
        1 => "&",
        2 => "//",
        3 => "<<",
        4 => "@",
        5 => "*",
        6 => "%",
        7 => "|",
        8 => "**",
        9 => ">>",
        10 => "-",
        11 => "/",
        12 => "^",
        _ => "unknown binary operation",
    }
}
