use crate::marshal::MarshalValue;

#[derive(Debug, Clone, PartialEq)]
pub struct CodeObject {
    pub arg_count: u32,
    pub positional_only_arg_count: u32,
    pub keyword_only_arg_count: u32,
    pub stack_size: u32,
    pub flags: u32,
    pub bytecode: Vec<u8>,
    pub constants: Vec<MarshalValue>,
    pub names: Vec<String>,
    pub localsplus_names: Vec<String>,
    pub localsplus_kinds: Vec<u8>,
    pub filename: String,
    pub name: String,
    pub qualified_name: String,
    pub first_line_number: u32,
    pub line_table: Vec<u8>,
    pub exception_table: Vec<u8>,
}

impl CodeObject {
    pub fn nested_code_objects(&self) -> impl Iterator<Item = &CodeObject> {
        self.constants.iter().filter_map(|value| match value {
            MarshalValue::Code(code) => Some(code.as_ref()),
            _ => None,
        })
    }
}
