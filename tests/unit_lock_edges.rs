use aifo_coder as aifo;
use std::{thread, time::Duration};

#[test]
fn unit_concurrent_locking_threaded() {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "aifo-coder-lock-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    // Acquire and hold first lock
    let f1 = aifo::acquire_lock_at(&p).expect("first acquire_lock_at failed");

    // Second attempt in another thread must fail while lock is held
    let p2 = p.clone();
    let handle = thread::spawn(move || {
        let e = aifo::acquire_lock_at(&p2).expect_err("second lock unexpectedly succeeded");
        assert_eq!(e.kind(), std::io::ErrorKind::Other);
        assert!(
            e.to_string().contains("already running"),
            "unexpected error message: {e}"
        );
    });

    // Give spawned thread time to attempt lock
    thread::sleep(Duration::from_millis(50));
    drop(f1);
    handle.join().unwrap();

    // After release, acquiring should succeed again
    let f3 = aifo::acquire_lock_at(&p).expect("lock after release should succeed");
    drop(f3);

    // cleanup
    let _ = std::fs::remove_file(&p);
}
