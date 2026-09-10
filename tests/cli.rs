use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn compiles_a_python_script_and_runs_print() {
    let version = Command::new("python3")
        .arg("--version")
        .output()
        .expect("python3 is required for the CLI integration test");
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
    let script_path =
        std::env::temp_dir().join(format!("rython-cli-{}-{unique}.py", std::process::id()));
    fs::write(
        &script_path,
        "value = 1 + 2\nprint(\"value:\", value)\nprint(True, None)\n",
    )
    .expect("write temporary Python script");

    let output = Command::new(env!("CARGO_BIN_EXE_rython"))
        .arg(&script_path)
        .output()
        .expect("run rython executable");

    let _ = fs::remove_file(&script_path);
    assert!(
        output.status.success(),
        "rython failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "value: 3\nTrue None\n"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("[rython] result: None"),
        "missing VM result in stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
