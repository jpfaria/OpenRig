//! Lookup table mapping `Backend` variants to disk-package builders.
//!
//! Each backend crate (`nam`, `ir`, `lv2`, `vst3-host`) registers its
//! `build_from_package` fn here at process startup. `LoadedPackage::build_processor`
//! reads the table and dispatches without needing to know which backends
//! are compiled in — keeps `plugin-loader` free of audio dependencies
//! and keeps the consumer crates (block-*) from having to match on
//! [`crate::manifest::Backend`] themselves.
//!
//! Issue: #287

use std::sync::Mutex;

use anyhow::Result;
use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::discover::LoadedPackage;
use crate::manifest::Backend;

/// `(package, params, sample_rate, layout) -> BlockProcessor` — same
/// shape every backend crate exposes. Plain fn pointer so the table
/// fits in a `Mutex<[Option<...>]>`.
pub type PackageBuildFn =
    fn(&LoadedPackage, &ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>;

/// One slot per non-Native variant of [`Backend`]. Native lives in
/// [`crate::native_runtimes`] (keyed by `runtime_id`, not backend kind),
/// so it has no entry here.
#[derive(Default)]
struct Builders {
    nam: Option<PackageBuildFn>,
    ir: Option<PackageBuildFn>,
    lv2: Option<PackageBuildFn>,
    vst3: Option<PackageBuildFn>,
}

static BUILDERS: Mutex<Builders> = Mutex::new(Builders {
    nam: None,
    ir: None,
    lv2: None,
    vst3: None,
});

fn with<R>(work: impl FnOnce(&mut Builders) -> R) -> R {
    let mut guard = BUILDERS.lock().expect("package_builders mutex poisoned");
    work(&mut guard)
}

/// Discriminant kind paired with [`Backend`] variants. Used to register
/// or look up a builder without exposing the wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackendKind {
    Nam,
    Ir,
    Lv2,
    Vst3,
}

impl BackendKind {
    /// `None` for `Backend::Native` — those don't go through this table.
    pub fn from_backend(backend: &Backend) -> Option<Self> {
        match backend {
            Backend::Native { .. } => None,
            Backend::Nam { .. } => Some(BackendKind::Nam),
            Backend::Ir { .. } => Some(BackendKind::Ir),
            Backend::Lv2 { .. } => Some(BackendKind::Lv2),
            Backend::Vst3 { .. } => Some(BackendKind::Vst3),
        }
    }
}

/// Register the builder for `kind`. Called once per backend crate at
/// startup. Last writer wins per kind, but in practice each crate
/// registers a distinct backend so there's no race.
pub fn register(kind: BackendKind, build: PackageBuildFn) {
    with(|table| match kind {
        BackendKind::Nam => table.nam = Some(build),
        BackendKind::Ir => table.ir = Some(build),
        BackendKind::Lv2 => table.lv2 = Some(build),
        BackendKind::Vst3 => table.vst3 = Some(build),
    });
}

/// Look up the registered builder for `kind`. Returns `None` if no
/// crate registered one — callers must surface a clear error rather
/// than silently failing.
pub fn get(kind: BackendKind) -> Option<PackageBuildFn> {
    with(|table| match kind {
        BackendKind::Nam => table.nam,
        BackendKind::Ir => table.ir,
        BackendKind::Lv2 => table.lv2,
        BackendKind::Vst3 => table.vst3,
    })
}
