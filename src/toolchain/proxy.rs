/*!
Proxy module: exposes the public toolexec_start_proxy API.

The implementation lives in toolchain.rs as toolexec_start_proxy_impl; this
module provides a stable facade and keeps external imports unchanged.

Phase 1 (v3 spec) status:
- Endpoint normalization in place: only "/exec" and "/notify" are recognized.
- Deduplication done: auth and form parsing centralized (auth::…, http::…),
  notifications logic moved to toolchain::notifications.
- Unified form decoding (+ and %XX) implemented in http::parse_form_urlencoded.
- 1 MiB form body cap enforced; oversized requests map to 400 Bad Request.
- Phase 1 complete.
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
