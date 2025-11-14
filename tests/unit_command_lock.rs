use aifo_coder as aifo;
use std::io;


#[test]
fn unit_acquire_lock_at_exclusive_and_release() {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "aifo-coder-lock-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    // First lock should succeed
    let f1 = aifo::acquire_lock_at(&p).expect("first acquire_lock_at failed");
    // Second lock on same path should fail
    let e = aifo::acquire_lock_at(&p).expect_err("second acquire_lock_at unexpectedly succeeded");
    assert_eq!(e.kind(), io::ErrorKind::Other);
    assert!(
        e.to_string().contains("already running"),
        "unexpected error message: {e}"
    );
    drop(f1);
    // After releasing, should succeed again
    let _f2 = aifo::acquire_lock_at(&p).expect("acquire_lock_at after release failed");
    // cleanup
    let _ = std::fs::remove_file(&p);
}
