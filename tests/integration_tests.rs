use std::process::Command;

fn run_pyrs(input: &str, output: &str) -> i32 {
    // Build and run using the 'run' command
    // We ignore the output and run the binary ourselves to get the exit code easily
    let status = Command::new("cargo")
        .args(&["run", "--", "run", input, "--output", output])
        .status()
        .expect("Failed to execute cargo run");
    
    if !status.success() {
        panic!("pyrs run failed for {}", input);
    }

    let bin_path = format!("bin/{}", output);
    let run_status = Command::new(format!("./{}", bin_path))
        .status()
        .expect("Failed to execute compiled binary");
    
    run_status.code().unwrap_or(-1)
}

#[test]
fn test_fibonacci() {
    assert_eq!(run_pyrs("examples/fibonacci.pyrs", "test_fib"), 55);
}

#[test]
fn test_factorial() {
    assert_eq!(run_pyrs("examples/factorial.pyrs", "test_fact"), 120);
}

#[test]
fn test_loop() {
    assert_eq!(run_pyrs("examples/loop.pyrs", "test_loop"), 45);
}

#[test]
fn test_logic() {
    assert_eq!(run_pyrs("examples/logic.pyrs", "test_logic"), 1);
}
