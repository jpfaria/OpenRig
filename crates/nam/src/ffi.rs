//! Responsibility: declares the C functions the NAM library exposes.

use std::os::raw::{c_int, c_void};

use crate::plugin_config::NamPluginConfig;

// The build script (`crates/nam/build.rs`) links the cmake-built
// `libnam_wrapper` on every platform, so a plain `extern "C"` is enough
// — no per-OS `raw-dylib`/import-library handling is required.
unsafe extern "C" {
    pub(crate) fn nam_create(config: *const NamPluginConfig) -> *mut c_void;
    pub(crate) fn nam_destroy(handle: *mut c_void);
    // The C symbol is `nam_process`; the Rust ident is renamed so the
    // public, slice-based `nam_process` diagnostics wrapper below can keep
    // the historical name (issue #623 req #2). Same FFI, no ABI change.
    #[link_name = "nam_process"]
    pub(crate) fn nam_process_ffi(
        handle: *mut c_void,
        input: *const f32,
        output: *mut f32,
        nframes: c_int,
    );
}
