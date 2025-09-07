/*!
Toolchain orchestration module.

This module re-exports the public toolchain APIs from the crate root to establish
a clear boundary for sidecars, proxy, and notifications logic as part of Phase 4.
*/

pub use crate::{
    normalize_toolchain_kind,
    default_toolchain_image_for_version,
    toolchain_write_shims,
    toolchain_start_session,
    toolchain_cleanup_session,
    toolchain_purge_caches,
    toolchain_run,
    route_tool_to_sidecar,
    toolexec_start_proxy,
    toolchain_bootstrap_typescript_global,
};
