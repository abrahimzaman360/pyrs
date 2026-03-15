mod common;
use common::run_pyrs_ext;

#[test]
fn test_list_operations() {
    let result = run_pyrs_ext("examples/list_ops.pyrs", "test_list_ops", &["--gc", "off"]);
    
    assert_eq!(result.status, 0);
    assert!(result.stdout.contains("l[0]: 1"));
    assert!(result.stdout.contains("l[1]: 2"));
    assert!(result.stdout.contains("l[2]: 3"));
    assert!(result.stdout.contains("After set:"));
    assert!(result.stdout.contains("l[0]: 10"));
    assert!(result.stdout.contains("l[1]: 20"));
    assert!(result.stdout.contains("l[2]: 30"));
}

#[test]
fn test_list_operations_gc_dyn() {
    let result = run_pyrs_ext("examples/list_ops.pyrs", "test_list_ops_dyn", &["--gc", "dyn"]);
    
    assert_eq!(result.status, 0);
    assert!(result.stdout.contains("l[0]: 1"));
    assert!(result.stdout.contains("l[2]: 30"));
}
