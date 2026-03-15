use std::fs;
use std::process::Command;

#[allow(unused)]
pub struct TestResult {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

pub fn run_pyrs_ext(input: &str, output: &str, extra_args: &[&str]) -> TestResult {
    // Ensure build directories exist
    let _ = fs::create_dir_all(".buildout");
    let _ = fs::create_dir_all("bin");

    // Build and run the program using the compiler's sub-command
    // Since main.rs now exits with the program status, we can get it from cargo run.
    let mut args = vec!["run", "-q", "--", "run", input, "--output", output];
    args.extend_from_slice(extra_args);

    let output_res = Command::new("cargo")
        .args(&args)
        .output()
        .expect("Failed to execute cargo run");
    
    TestResult {
        status: output_res.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output_res.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output_res.stderr).to_string(),
    }
}
