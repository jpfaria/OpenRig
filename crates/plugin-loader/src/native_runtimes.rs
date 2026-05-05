//! In-binary lookup table for native plugin runtimes.
//!
//! Native plugins live as Rust DSP code in their `block-*` crate; the
//! manifest in [`crate::registry`] only carries metadata (id, display name,
//! parameter schema). The actual `build`, `validate`, and `schema` fn
//! pointers register themselves here at process startup, keyed by
//! `runtime_id` (the `Backend::Native { runtime_id }` value).
//!
//! Registration happens during boot — each `block-*` crate's
//! `register_natives()` pushes one [`NativeRuntime`] per native model,
//! then the same call also pushes its synthesized `PluginManifest` into
//! [`crate::registry`]. After boot the table is read-only from the
//! perspective of any caller (GUI, engine), so the [`Mutex`] never
//! contends in the hot path.
//!
//! Issue: #287

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

/// Build a [`BlockProcessor`] from a parameter set. Same signature every
/// `block-*` crate's `MODEL_DEFINITION.build` already uses.
pub type NativeBuildFn = fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>;

/// Validate a parameter set against the native model's constraints.
pub type NativeValidateFn = fn(&ParameterSet) -> Result<()>;

/// Return the parameter schema this native model exposes (ranges,
/// defaults, units). Derived from constants inside the model crate.
pub type NativeSchemaFn = fn() -> Result<ModelParameterSchema>;

/// Bundle of fn pointers a native plugin registers at startup.
#[derive(Clone, Copy)]
pub struct NativeRuntime {
    pub schema: NativeSchemaFn,
    pub validate: NativeValidateFn,
    pub build: NativeBuildFn,
}

static RUNTIMES: Mutex<Option<HashMap<String, NativeRuntime>>> = Mutex::new(None);

fn with_table<R>(work: impl FnOnce(&mut HashMap<String, NativeRuntime>) -> R) -> R {
    let mut guard = RUNTIMES.lock().expect("native_runtimes mutex poisoned");
    let table = guard.get_or_insert_with(HashMap::new);
    work(table)
}

/// Register a native runtime. Last writer wins for a given `runtime_id`,
/// but in practice each id is unique to one model so there's no race.
pub fn register(runtime_id: &str, runtime: NativeRuntime) {
    with_table(|table| {
        table.insert(runtime_id.to_string(), runtime);
    });
}

/// Look up a runtime by id. Returns `None` when the id was never
/// registered (typically a manifest referring to a model whose crate
/// didn't ship in this build).
pub fn get(runtime_id: &str) -> Option<NativeRuntime> {
    with_table(|table| table.get(runtime_id).copied())
}

/// Number of native runtimes registered. Useful for boot-time logging.
pub fn count() -> usize {
    with_table(|table| table.len())
}
