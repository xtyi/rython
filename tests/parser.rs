use rython::{
    parse_code_object, parse_header, parse_marshal, parse_pyc, MarshalValue, PycValidation,
    PYTHON_3_12_MAGIC,
};
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn hex(input: &str) -> Vec<u8> {
    input
        .split_whitespace()
        .map(|part| u8::from_str_radix(part, 16).expect("valid hex fixture"))
        .collect()
}

fn timestamp_pyc(payload: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(16 + payload.len());
    result.extend_from_slice(&PYTHON_3_12_MAGIC);
    result.extend_from_slice(&0u32.to_le_bytes());
    result.extend_from_slice(&123u32.to_le_bytes());
    result.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    result.extend_from_slice(payload);
    result
}

const MODULE_PAYLOAD: &str = "\
e3 00 00 00 00 00 00 00 00 00 00 00 00 02 00 00 \
00 00 00 00 00 f3 10 00 00 00 97 00 64 00 5a 00 \
65 00 64 01 66 02 5a 01 79 02 29 03 e9 01 00 00 \
00 e9 02 00 00 00 4e 29 02 da 01 78 da 01 79 a9 00 \
f3 00 00 00 00 fa 09 3c 66 69 78 74 75 72 65 3e \
fa 08 3c 6d 6f 64 75 6c 65 3e 72 09 00 00 00 01 00 \
00 00 73 13 00 00 00 f0 03 01 01 01 d8 04 05 80 01 \
d8 05 06 88 01 80 46 81 01 72 07 00 00 00";

const NESTED_FUNCTION_PAYLOAD: &str = "\
e3 00 00 00 00 00 00 00 00 00 00 00 00 02 00 00 00 \
00 00 00 00 f3 0c 00 00 00 97 00 64 03 64 01 84 01 5a \
00 79 02 29 04 e9 02 00 00 00 63 02 00 00 00 00 00 00 \
00 00 00 00 00 02 00 00 00 03 00 00 00 f3 0c 00 00 00 \
97 00 7c 00 7c 01 7a 00 00 00 53 00 29 01 4e a9 00 29 \
02 da 01 61 da 01 62 73 02 00 00 00 20 20 fa 09 3c 66 \
69 78 74 75 72 65 3e da 03 61 64 64 72 08 00 00 00 01 \
00 00 00 73 0b 00 00 00 80 00 d8 0b 0c 88 71 89 35 80 \
4c f3 00 00 00 00 4e 29 01 72 02 00 00 00 29 01 72 08 \
00 00 00 72 04 00 00 00 72 09 00 00 00 72 07 00 00 00 \
fa 08 3c 6d 6f 64 75 6c 65 3e 72 0a 00 00 00 01 00 00 \
00 73 0a 00 00 00 f0 03 01 01 01 f4 02 01 01 11 72 09 \
00 00 00";

#[test]
fn parses_python_312_pyc_header_and_module_code() {
    let payload = hex(MODULE_PAYLOAD);
    let file = parse_pyc(&timestamp_pyc(&payload)).expect("valid Python 3.12 .pyc");

    assert_eq!(file.header.magic, PYTHON_3_12_MAGIC);
    assert_eq!(
        file.header.validation,
        PycValidation::Timestamp {
            timestamp: 123,
            source_size: payload.len() as u32
        }
    );
    assert_eq!(file.code.arg_count, 0);
    assert_eq!(file.code.stack_size, 2);
    assert_eq!(file.code.bytecode.len(), 16);
    assert_eq!(file.code.filename, "<fixture>");
    assert_eq!(file.code.name, "<module>");
    assert_eq!(file.code.qualified_name, "<module>");
    assert_eq!(file.code.constants[0], MarshalValue::Int(1));
    assert_eq!(file.code.constants[1], MarshalValue::Int(2));
    assert_eq!(file.code.constants[2], MarshalValue::None);
    assert_eq!(file.code.names, vec!["x", "y"]);
}

#[test]
fn parses_nested_python_312_code_objects() {
    let code = parse_code_object(&hex(NESTED_FUNCTION_PAYLOAD)).expect("nested code object");
    let nested = code
        .nested_code_objects()
        .next()
        .expect("nested function code");

    assert_eq!(nested.name, "add");
    assert_eq!(nested.qualified_name, "add");
    assert_eq!(nested.arg_count, 2);
    assert_eq!(nested.stack_size, 2);
    assert_eq!(nested.localsplus_names, vec!["a", "b"]);
    assert_eq!(nested.localsplus_kinds, vec![0x20, 0x20]);
    assert_eq!(nested.constants, vec![MarshalValue::None]);
}

#[test]
fn parses_a_pyc_generated_by_cpython_312() {
    let version = Command::new("python3")
        .arg("--version")
        .output()
        .expect("python3 is required for this integration test");
    let version_text = format!(
        "{}{}",
        String::from_utf8_lossy(&version.stdout),
        String::from_utf8_lossy(&version.stderr)
    );
    if !version_text.starts_with("Python 3.12.") {
        return;
    }

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after Unix epoch")
        .as_nanos();
    let base = std::env::temp_dir().join(format!("rython-parser-{}-{unique}", std::process::id()));
    let source_path = base.with_extension("py");
    let pyc_path = base.with_extension("pyc");
    fs::write(&source_path, "def add(a, b=2):\n    return a + b\n")
        .expect("write temporary Python source");

    let compile = Command::new("python3")
        .args([
            "-c",
            "import py_compile, sys; py_compile.compile(sys.argv[1], cfile=sys.argv[2], doraise=True)",
            source_path.to_str().expect("UTF-8 temporary source path"),
            pyc_path.to_str().expect("UTF-8 temporary pyc path"),
        ])
        .output()
        .expect("run CPython py_compile");
    assert!(
        compile.status.success(),
        "py_compile failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let pyc = fs::read(&pyc_path).expect("read generated .pyc");
    let file = parse_pyc(&pyc).expect("parse CPython 3.12 .pyc");
    assert_eq!(file.header.magic, PYTHON_3_12_MAGIC);
    assert_eq!(file.code.name, "<module>");
    assert_eq!(
        file.code
            .nested_code_objects()
            .next()
            .expect("nested add function")
            .name,
        "add"
    );

    let _ = fs::remove_file(source_path);
    let _ = fs::remove_file(pyc_path);
}

#[test]
fn parses_hash_based_header() {
    let mut input = vec![0u8; 16];
    input[0..4].copy_from_slice(&PYTHON_3_12_MAGIC);
    input[4..8].copy_from_slice(&3u32.to_le_bytes());
    input[8..16].copy_from_slice(b"12345678");

    let (header, offset) = parse_header(&input).expect("valid hash-based header");
    assert_eq!(offset, 16);
    assert_eq!(
        header.validation,
        PycValidation::Hash {
            hash: *b"12345678",
            check_source: true
        }
    );
}

#[test]
fn resolves_marshal_reference_table_entries() {
    let payload = hex("a9 02 fa 01 78 72 01 00 00 00");
    let value = parse_marshal(&payload).expect("valid tuple with a reference");

    assert_eq!(
        value,
        MarshalValue::Tuple(vec![
            MarshalValue::String("x".to_string()),
            MarshalValue::String("x".to_string()),
        ])
    );
}

#[test]
fn parses_basic_marshal_values() {
    assert_eq!(
        parse_marshal(&hex("69 00 00 00 00")).expect("int"),
        MarshalValue::Int(0)
    );
    assert_eq!(parse_marshal(&hex("4e")).expect("none"), MarshalValue::None);
    assert_eq!(
        parse_marshal(&hex("66 03 31 2e 35")).expect("float"),
        MarshalValue::Float(1.5)
    );
    assert_eq!(
        parse_marshal(&hex("e7 00 00 00 00 00 00 f8 3f")).expect("binary float"),
        MarshalValue::Float(1.5)
    );
    assert_eq!(
        parse_marshal(&hex("ec 03 00 00 00 00 00 00 00 00 04")).expect("long"),
        MarshalValue::Long(rython::MarshalLong {
            negative: false,
            digits: vec![0, 0, 1024],
        })
    );
    assert_eq!(
        parse_marshal(&hex("73 03 00 00 00 61 62 63")).expect("bytes"),
        MarshalValue::Bytes(b"abc".to_vec())
    );
    assert_eq!(
        parse_marshal(&hex("7a 03 61 62 63")).expect("short ascii"),
        MarshalValue::String("abc".to_string())
    );
}

#[test]
fn rejects_wrong_magic_and_trailing_payload() {
    let payload = hex(MODULE_PAYLOAD);
    let mut input = timestamp_pyc(&payload);
    input[0] = 0;
    assert!(parse_pyc(&input).is_err());

    let mut payload_with_trailing = payload;
    payload_with_trailing.push(0);
    assert!(parse_code_object(&payload_with_trailing).is_err());
}
