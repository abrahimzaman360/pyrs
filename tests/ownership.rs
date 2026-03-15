mod common;
use common::run_pyrs_ext;

#[test]
fn test_ownership_valid() {
    let result = run_pyrs_ext("examples/ownership_valid.pyrs", "test_own_valid", &["--gc", "on"]);
    assert_eq!(result.status, 0, "Valid ownership code should compile and run. Stderr: {}", result.stderr);
    assert!(result.stdout.contains("y[0]: 1"));
    assert!(result.stdout.contains("z[1]: 2"));
    assert!(result.stdout.contains("w[2]: 3"));
}

#[test]
fn test_ownership_invalid_move() {
    let result = run_pyrs_ext("examples/ownership_invalid_move.pyrs", "test_own_inv_move", &["--gc", "on"]);
    assert_ne!(result.status, 0, "Invalid move should fail compilation");
    assert!(result.stderr.contains("Use of moved value: 'x'"), "Should report use of moved value error. Stderr: {}", result.stderr);
}

#[test]
fn test_ownership_invalid_borrow() {
    let result = run_pyrs_ext("examples/ownership_invalid_borrow.pyrs", "test_own_inv_borrow", &["--gc", "on"]);
    assert_ne!(result.status, 0, "Invalid borrow should fail compilation");
    assert!(result.stderr.contains("already borrowed"), "Should report borrow violation error. Stderr: {}", result.stderr);
}

#[test]
fn test_ownership_disabled_off() {
    // With --gc off, the same 'invalid' move should compile fine (because rules are not enforced)
    let result = run_pyrs_ext("examples/ownership_invalid_move.pyrs", "test_own_off", &["--gc", "off"]);
    assert_eq!(result.status, 0, "Invalid move should compile fine with --gc off. Stderr: {}", result.stderr);
}
