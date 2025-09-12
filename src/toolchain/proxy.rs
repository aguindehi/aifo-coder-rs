/*!
Proxy module: exposes the public toolexec_start_proxy API.

The implementation lives in toolchain.rs as toolexec_start_proxy_impl; this
module provides a stable facade and keeps external imports unchanged.
*/
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
