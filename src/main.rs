use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    if let Err(error) = run() {
        eprintln!("rython: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os();
    let program = args.next().unwrap_or_else(|| OsString::from("rython"));
    let Some(script_path) = args.next() else {
        return Err(usage_error(&program));
    };
    if args.next().is_some() {
        return Err(usage_error(&program));
    }

    let script_path = PathBuf::from(script_path);
    if !script_path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Python script does not exist: {}", script_path.display()),
        )
        .into());
    }

    let pyc = compile_to_pyc(&script_path)?;
    let pyc = rython::parse_pyc(&pyc)?;
    let mut vm = rython::Vm::new();
    let result = vm.run_pyc(&pyc)?;

    let output = vm.take_output();
    let mut stdout = io::stdout().lock();
    stdout.write_all(output.as_bytes())?;
    stdout.flush()?;

    eprintln!("[rython] result: {result}");
    Ok(())
}

fn usage_error(program: &OsString) -> Box<dyn Error> {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("usage: {} <script.py>", Path::new(program).display()),
    )
    .into()
}

fn compile_to_pyc(script_path: &Path) -> Result<Vec<u8>, Box<dyn Error>> {
    let python = env::var_os("RYTHON_PYTHON").unwrap_or_else(|| OsString::from("python3"));
    let pyc_path = temporary_pyc_path();
    let output = Command::new(&python)
        .arg("-c")
        .arg(
            "import py_compile, sys; \
             py_compile.compile(sys.argv[1], cfile=sys.argv[2], doraise=True)",
        )
        .arg(script_path)
        .arg(&pyc_path)
        .output()?;

    if !output.status.success() {
        let _ = fs::remove_file(&pyc_path);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(io::Error::other(format!(
            "Python frontend failed with status {}:\n{}{}",
            output.status, stdout, stderr
        ))
        .into());
    }

    let pyc = fs::read(&pyc_path)?;
    let _ = fs::remove_file(&pyc_path);
    Ok(pyc)
}

fn temporary_pyc_path() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after Unix epoch")
        .as_nanos();
    env::temp_dir().join(format!("rython-{}-{unique}.pyc", std::process::id()))
}
