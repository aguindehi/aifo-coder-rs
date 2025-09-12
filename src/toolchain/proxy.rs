use std::io;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread::JoinHandle;

pub fn toolexec_start_proxy(
    session_id: &str,
    verbose: bool,
) -> io::Result<(String, String, Arc<AtomicBool>, JoinHandle<()>)> {
    super::toolexec_start_proxy_impl(session_id, verbose)
}
