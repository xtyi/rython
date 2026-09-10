use rython::{
    decode, parse_pyc, BytecodeError, Instruction, Opcode, PycFile, Vm, VmValue, PYTHON_3_12_MAGIC,
};
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn compile_source(source: &str) -> Option<PycFile> {
    let version = Command::new("python3")
        .arg("--version")
        .output()
        .expect("python3 is required for VM integration tests");
    let version_text = format!(
        "{}{}",
        String::from_utf8_lossy(&version.stdout),
        String::from_utf8_lossy(&version.stderr)
    );
    if !version_text.starts_with("Python 3.12.") {
        return None;
    }

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after Unix epoch")
        .as_nanos();
    let base = std::env::temp_dir().join(format!("rython-vm-{}-{unique}", std::process::id()));
    let source_path = base.with_extension("py");
    let pyc_path = base.with_extension("pyc");
    fs::write(&source_path, source).expect("write Python source");

    let compile = Command::new("python3")
        .args([
            "-c",
            "import py_compile, sys; py_compile.compile(sys.argv[1], cfile=sys.argv[2], doraise=True)",
            source_path.to_str().expect("UTF-8 source path"),
            pyc_path.to_str().expect("UTF-8 pyc path"),
        ])
        .output()
        .expect("run CPython py_compile");
    assert!(
        compile.status.success(),
        "py_compile failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let bytes = fs::read(&pyc_path).expect("read .pyc");
    let result = parse_pyc(&bytes).expect("parse generated .pyc");
    let _ = fs::remove_file(source_path);
    let _ = fs::remove_file(pyc_path);
    Some(result)
}

#[test]
fn decodes_python_312_wordcode_and_extended_args() {
    let instructions = decode(&[
        151, 0, // RESUME 0
        144, 1, // EXTENDED_ARG 1
        100, 2, // LOAD_CONST 0x102
        0, 0, // CACHE
    ])
    .expect("valid wordcode");

    assert_eq!(
        instructions,
        vec![
            Instruction {
                offset: 0,
                opcode: Opcode::Resume,
                arg: 0,
                size: 2,
            },
            Instruction {
                offset: 2,
                opcode: Opcode::LoadConst,
                arg: 0x102,
                size: 4,
            },
            Instruction {
                offset: 6,
                opcode: Opcode::Cache,
                arg: 0,
                size: 2,
            },
        ]
    );
}

#[test]
fn rejects_malformed_wordcode() {
    assert_eq!(
        decode(&[151]).expect_err("odd-length bytecode must fail"),
        BytecodeError::OddLength { length: 1 }
    );
    assert_eq!(
        decode(&[144, 1]).expect_err("dangling EXTENDED_ARG must fail"),
        BytecodeError::DanglingExtendedArg { offset: 0 }
    );
}

#[test]
fn executes_integer_float_string_and_comparison_operations() {
    let Some(pyc) = compile_source(
        r#"
a = 10
b = 3
add = a + b
sub = a - b
mul = a * b
floor = a // b
remainder = a % b
true_div = a / b
power = b ** 2
left_shift = b << 2
bit_and = a & b
bit_or = a | b
bit_xor = a ^ b
negative = -b
positive = +b
less = a < b
equal = a == 10
not_equal = a != b
text = "a" + "b"
mixed = 1.5 + 2
"#,
    ) else {
        return;
    };

    let mut vm = Vm::new();
    assert_eq!(vm.run_pyc(&pyc).expect("execute arithmetic"), VmValue::None);
    assert_eq!(vm.global("add"), Some(&VmValue::Int(13)));
    assert_eq!(vm.global("sub"), Some(&VmValue::Int(7)));
    assert_eq!(vm.global("mul"), Some(&VmValue::Int(30)));
    assert_eq!(vm.global("floor"), Some(&VmValue::Int(3)));
    assert_eq!(vm.global("remainder"), Some(&VmValue::Int(1)));
    assert_eq!(vm.global("true_div"), Some(&VmValue::Float(10.0 / 3.0)));
    assert_eq!(vm.global("power"), Some(&VmValue::Int(9)));
    assert_eq!(vm.global("left_shift"), Some(&VmValue::Int(12)));
    assert_eq!(vm.global("bit_and"), Some(&VmValue::Int(2)));
    assert_eq!(vm.global("bit_or"), Some(&VmValue::Int(11)));
    assert_eq!(vm.global("bit_xor"), Some(&VmValue::Int(9)));
    assert_eq!(vm.global("negative"), Some(&VmValue::Int(-3)));
    assert_eq!(vm.global("positive"), Some(&VmValue::Int(3)));
    assert_eq!(vm.global("less"), Some(&VmValue::Bool(false)));
    assert_eq!(vm.global("equal"), Some(&VmValue::Bool(true)));
    assert_eq!(vm.global("not_equal"), Some(&VmValue::Bool(true)));
    assert_eq!(vm.global("text"), Some(&VmValue::String("ab".to_string())));
    assert_eq!(vm.global("mixed"), Some(&VmValue::Float(3.5)));
}

#[test]
fn executes_containers_subscripts_and_assignment() {
    let Some(pyc) = compile_source(
        r#"
a = 1
b = 2
values = [a, b, 3]
second = values[1]
values[1] = 9
pair = (a, b)
first = pair[0]
mapping = {"a": a}
mapped = mapping["a"]
mapping["b"] = b
items = {a, b, a}
combined = "x" * 3
"#,
    ) else {
        return;
    };

    let mut vm = Vm::new();
    vm.run_pyc(&pyc).expect("execute containers");

    assert_eq!(
        vm.global("values"),
        Some(&VmValue::List(vec![
            VmValue::Int(1),
            VmValue::Int(9),
            VmValue::Int(3),
        ]))
    );
    assert_eq!(vm.global("second"), Some(&VmValue::Int(2)));
    assert_eq!(
        vm.global("pair"),
        Some(&VmValue::Tuple(vec![VmValue::Int(1), VmValue::Int(2)]))
    );
    assert_eq!(vm.global("first"), Some(&VmValue::Int(1)));
    assert_eq!(vm.global("mapped"), Some(&VmValue::Int(1)));
    assert_eq!(
        vm.global("mapping"),
        Some(&VmValue::Dict(vec![
            (VmValue::String("a".to_string()), VmValue::Int(1)),
            (VmValue::String("b".to_string()), VmValue::Int(2)),
        ]))
    );
    assert_eq!(
        vm.global("items"),
        Some(&VmValue::Set(vec![VmValue::Int(1), VmValue::Int(2)]))
    );
    assert_eq!(
        vm.global("combined"),
        Some(&VmValue::String("xxx".to_string()))
    );
}

#[test]
fn executes_conditional_jumps() {
    let Some(pyc) = compile_source(
        r#"
x = 1
limit = 2
if x < limit:
    result = "yes"
else:
    result = "no"
"#,
    ) else {
        return;
    };

    let mut vm = Vm::new();
    vm.run_pyc(&pyc).expect("execute branch");
    assert_eq!(
        vm.global("result"),
        Some(&VmValue::String("yes".to_string()))
    );
}

#[test]
fn executes_local_load_and_store_in_a_nested_code_object() {
    let Some(pyc) = compile_source(
        r#"
def add(a, b):
    value = a + b
    return value
"#,
    ) else {
        return;
    };
    assert_eq!(pyc.header.magic, PYTHON_3_12_MAGIC);

    let code = pyc.code.nested_code_objects().next().expect("add code");
    let mut vm = Vm::new();
    assert_eq!(
        vm.run_code_with_locals(code, vec![VmValue::Int(4), VmValue::Int(5)])
            .expect("execute local function body"),
        VmValue::Int(9)
    );
}
