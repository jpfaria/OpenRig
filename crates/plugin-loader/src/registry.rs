//! Responsibility: holds every plugin known at runtime.
//! Process-wide registry of every plugin known at runtime.
//!
//! Two ways a plugin enters the registry:
//!
//! - **Disk packages** — `init(plugins_root)` (or `init_many`) runs
//!   `discover()` once at startup and pushes one [`LoadedPackage`] per
//!   `manifest.yaml` found. `reload(plugins_roots)` re-runs the disk
//!   scan after a new pack lands on disk so the running process picks
//!   it up without a restart (issue #561).
//! - **Native plugins** — each `block-*` crate calls
//!   [`register_native`] for each compiled-in DSP model, supplying a
//!   synthesized [`PluginManifest`] (with `Backend::Native { runtime_id }`)
//!   plus the runtime fn pointers that go into
//!   [`crate::native_runtimes`].
//!
//! Native registration happens **before** [`init`] / [`init_many`] is
//! called. The native list is kept in a separate static so [`reload`]
//! can rebuild the disk side without losing the natives (they cannot
//! be re-discovered — they have no manifest on disk). Every call to
//! `reload` re-scans the disk roots and atomically swaps the public
//! `&'static [LoadedPackage]` slice; old references taken before the
//! swap stay valid (the previous slice is leaked, not freed), so any
//! cached `&'static LoadedPackage` survives the reload.
//!
//! Issues: #287, #561

use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, RwLock};

use crate::discover::LoadedPackage;

/// Persistent list of natives. Populated once at startup by
/// `block-*::register_natives`; never drained. [`reload`] reads this
/// each time it rebuilds the public registry so the natives are not
/// lost when re-scanning disk roots.
pub(crate) static NATIVES: Mutex<Vec<LoadedPackage>> = Mutex::new(Vec::new());

/// The currently published catalog. Always points at a leaked, immutable
/// slice — readers get `&'static` references that survive subsequent
/// reloads (the previous slice is intentionally not freed).
pub(crate) static REGISTRY: RwLock<&'static [LoadedPackage]> = RwLock::new(&[]);

/// Tracks whether [`init_many`] has already taken over publishing the
/// catalog. Subsequent `init_many` calls are no-ops (matches the
/// pre-#561 `OnceLock` semantics); [`reload`] bypasses this flag.
pub(crate) static REGISTRY_INITIALIZED: AtomicBool = AtomicBool::new(false);
pub use crate::registry_edit::{load_one, unload, CatalogOpError};
pub use crate::registry_load::{init, init_many, reload, ReloadStats};
pub use crate::registry_natives::{register_native, register_native_simple};
pub use crate::registry_query::{find, len, model_available, native_count, packages, packages_for};
