use std::process::Command;

fn compile_and_run(input: &str, output: &str) -> i32 {
    let pyrs_bin = env!("CARGO_BIN_EXE_pyrs");

    let status = Command::new(pyrs_bin)
        .args(["run", input, "-o", output, "--gc", "off"])
        .status()
        .expect("Failed to execute pyrs");

    status.code().unwrap_or(-1)
}

#[test]
fn test_fibonacci() {
    assert_eq!(compile_and_run("examples/fibonacci.pyrs", "test_fib"), 55);
}

#[test]
fn test_factorial() {
    assert_eq!(compile_and_run("examples/factorial.pyrs", "test_fact"), 120);
}

#[test]
fn test_loop() {
    assert_eq!(compile_and_run("examples/loop.pyrs", "test_loop"), 45);
}

#[test]
fn test_logic() {
    assert_eq!(compile_and_run("examples/logic.pyrs", "test_logic"), 1);
}

#[test]
fn test_extern() {
    assert_eq!(compile_and_run("examples/extern_demo.pyrs", "test_extern"), 0);
}
