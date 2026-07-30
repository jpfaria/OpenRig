//! `LocalDispatcher` — in-process implementation of `CommandDispatcher`.
//!
//! Holds the project via `Rc<RefCell<Project>>` for interior mutability so
//! `dispatch` can take `&self` (required by the trait; callers may hold
//! multiple references to the same dispatcher or to the same project).
//!
//! `adapter-gui`'s `ProjectSession` shares its project handle with this
//! dispatcher so both sides always see the same `Project` data with no extra
//! sync step.
//!
//! `dispatch` groups commands by category and delegates each to the
//! `handle_*` method that owns it — one per sibling `local_dispatcher_<feature>`
//! module. The read accessors, the dependency-attach setters, and the shared
//! chain/block borrow helpers live in their own sibling modules too
//! (`local_dispatcher_queries` / `_attach` / `_access`), keeping this file to
//! the struct definition and construction (issue #792 single-responsibility
//! split).
//!
//! #127: the single `impl CommandDispatcher for LocalDispatcher` block (Rust
//! allows only one per (trait, type) pair — E0119 otherwise) lives in
//! `local_dispatcher_trait.rs`, not here — this file was already at the
//! line cap before the GUI-facing surface moved onto the trait, and that
//! impl block's `dispatch`/`poll_async_results` bodies are large enough on
//! their own that keeping them here left no room.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use domain::ids::ChainId;
use domain::io_binding::IoBinding;
use engine::DiPcm;
use project::project::Project;
use project::rig::RigProject;

use crate::di_loader::DiLoopSource;
use crate::event::Event;
use crate::runtime_control::RuntimeControl;

/// The rate the dispatcher reports when NO audio stream is running: before the
/// first stream opens, and again once the runtime stops.
///
/// It is a REFERENCE, never an assumption about a live stream — a running
/// stream always overwrites it through `attach_engine_sr` with the rate the
/// device actually negotiated (issue #723: nothing on the live path may bake
/// in a fixed rate). Consumers that draw or measure with no device present
/// (the EQ curve in the block editor, the latency probe on a stopped rig) need
/// SOME rate to work with, and this is the one they agree on.
pub const REFERENCE_SAMPLE_RATE: u32 = 48_000;
use crate::selection_state::SelectionState;
use crate::tone_doctor_report::{ToneReport, ToneRun};

/// In-process dispatcher backed by a shared `Project`.
///
/// Uses `Rc<RefCell<_>>` for interior mutability on the main (UI) thread.
/// This is NOT `Send` — see the note in `dispatcher.rs` about deferred
/// `Send + Sync` bounds.
pub struct LocalDispatcher {
    pub(crate) project: Rc<RefCell<Project>>,
    /// #436: the rig (presets/scenes) used to live only in the GUI and
    /// be mutated by hand in a wiring closure. It now lives behind the
    /// dispatcher so MIDI/MCP/GUI all go through `SelectionCommand::ApplyRigNav`.
    /// `None` for non-rig sessions (legacy projects) — set via
    /// [`Self::attach_rig`] at project load.
    pub(crate) rig: RefCell<Option<Rc<RefCell<RigProject>>>>,
    /// #555: filesystem directory where preset YAMLs live. Used by
    /// `Command::SaveChainPreset` / `DeleteChainPreset` so the
    /// dispatcher (not the GUI) owns the `fs::write` / `fs::remove_file`
    /// calls. `None` until the session attaches one via
    /// [`Self::attach_presets_path`]; preset I/O Commands error out
    /// cleanly until that happens.
    pub(crate) presets_path: RefCell<Option<PathBuf>>,
    /// #555: target path for `Command::SaveProject`. The dispatcher
    /// writes the `.openrig` (+ legacy `.yaml` sibling when the user-
    /// facing path is `.yaml`) itself instead of relying on the GUI to
    /// do `fs::write`. `None` until the session attaches one — preset
    /// dispatcher tests that don't exercise project save keep working
    /// unchanged.
    pub(crate) project_path: RefCell<Option<PathBuf>>,
    /// #555: target path for the project's sidecar `config.yaml`. The
    /// GUI used to compute this from `project_path.parent()` on save;
    /// the dispatcher now owns the resolution. `None` ⇒ derive from
    /// `project_path.parent().join("config.yaml")` at save time.
    pub(crate) config_path: RefCell<Option<PathBuf>>,
    /// #792 / ADR-0003: the per-machine SYSTEM config path — where the I/O
    /// binding registry persists. SEPARATE from `config_path` (the project
    /// sidecar): opening a project sets `config_path` to `<project>/config.yaml`,
    /// and the per-machine registry must NOT follow it there. `None` ⇒
    /// `FilesystemStorage::app_config_path()`; tests attach a temp path.
    pub(crate) io_config_path: RefCell<Option<PathBuf>>,
    /// #548: which chain / block the user has active on the Chains
    /// screen, plus snapshots of the toggle states. MIDI slots and the
    /// GUI both mutate this through `Command`s; `QueryKind::Selection`
    /// exposes it to MCP / gRPC. `Arc<RwLock<…>>` because the MIDI
    /// daemon thread reads it cross-thread.
    pub(crate) selection_state: Arc<RwLock<SelectionState>>,

    /// #614: ephemeral per-chain DI loop state — NEVER serialized into the
    /// project (persisting a DI source is a project-level concern tracked
    /// separately in #324). Each entry holds the original source enum and
    /// the decoded `Arc<DiPcm>` (un-resampled source) ready for the arm path
    /// to resample per output-stream rate (#749). The adapter-gui wiring
    /// (Task 6) calls `di_loop_for_chain` to retrieve it when
    /// `Event::ChainDiLoopEnabledChanged` fires.
    pub(crate) di_loop_state: RefCell<HashMap<ChainId, (DiLoopSource, Arc<DiPcm>)>>,

    /// #614: sample rate used for DI loop decoding + resampling.
    /// [`REFERENCE_SAMPLE_RATE`] while nothing is running; the adapter sets
    /// the real value via `attach_engine_sr` once the audio stream is running,
    /// and puts it back when the runtime stops (#127).
    pub(crate) engine_sr: RefCell<u32>,

    /// #693: completion channel for command work running on its own
    /// task (DI decode, catalog rescan, ...). Handlers spawn a task
    /// with a clone of the sender; `poll_async_results` (frontend
    /// tick) drains the receiver, applies state and emits the events.
    pub(crate) async_done_tx: std::sync::mpsc::Sender<AsyncDone>,
    pub(crate) async_done_rx: std::sync::mpsc::Receiver<AsyncDone>,
    /// #791: the last Tone Doctor run per chain — in flight, finished, or
    /// failed. `ApplyToneDoctorFix` reads the measured correction back from
    /// here (so the caller applies exactly what it was shown), and the read
    /// side serves the whole state: a transport that only reads has no other
    /// way to learn a run failed.
    pub(crate) tone_doctor_runs: RefCell<HashMap<ChainId, ToneRun>>,
    /// #791: the adapter's live-input source for the doctor. `None` until
    /// attached — a chain with a loaded DI is diagnosable without it.
    pub(crate) tone_doctor_input: RefCell<Option<ToneDoctorInput>>,
    /// #127: the effective per-machine I/O binding registry, SHARED with the
    /// frontend (`Rc`, same allocation — the `attach_rig` pattern). The CRUD
    /// handlers mutate it and persist it; `SetIoBindings` installs it into the
    /// runtime; the frontend's sync path re-installs the very same handle, so
    /// an edit issued off the GUI is not reverted at the next chain sync.
    /// `None` until the frontend attaches one — then nothing is installed,
    /// rather than wiping the runtime's registry with an empty list.
    pub(crate) io_bindings: RefCell<Option<Rc<RefCell<Vec<IoBinding>>>>>,
    /// #127: how runtime-control commands reach the frontend's audio runtime.
    /// `None` ⇒ this process hosts no runtime (MCP-only, tests): the commands
    /// still record their state and emit their events, they just have nothing
    /// to apply the change to.
    pub(crate) runtime_control: RefCell<Option<Rc<dyn RuntimeControl>>>,
}

/// Completed off-thread command work (#693).
pub(crate) enum AsyncDone {
    /// DI-loop decode: install into `di_loop_state` + emit the event.
    DiLoad(ChainId, DiLoopSource, Result<Arc<DiPcm>, String>),
    /// #791: Tone Doctor verdict — cache it (the apply reads it back) and
    /// emit the report.
    ToneDiagnosis(ChainId, Result<ToneReport, String>),
    /// Work whose state lives elsewhere (e.g. the global plugin
    /// registry): just surface the completion events.
    Events(Vec<Event>),
}

/// #791: the captured signal for one Tone Doctor run, produced off-thread
/// (a live tap fills over N seconds, a DI decode is a blocking read).
pub type ToneDoctorCapture = Box<dyn FnOnce() -> Option<(Vec<[f32; 2]>, f32)> + Send>;

/// #791: how the adapter offers the chain's live input to the doctor.
///
/// Called on the dispatching thread with the chain and the analysis window in
/// seconds; it must only *prepare* the capture (subscribing to the tap is
/// cheap) and hand back the blocking part as a `Send` closure. `None` means
/// this chain has no live signal right now.
pub type ToneDoctorInput = Box<dyn Fn(&ChainId, usize) -> Option<ToneDoctorCapture>>;

impl LocalDispatcher {
    /// Create a dispatcher that operates on the given shared `Project` handle.
    ///
    /// The caller (e.g. `adapter-gui`'s `ProjectSession`) should `Rc::clone`
    /// its own project handle and pass it here so both sides share the same
    /// allocation.
    pub fn new(project: Rc<RefCell<Project>>) -> Self {
        let (async_done_tx, async_done_rx) = std::sync::mpsc::channel();
        Self {
            project,
            rig: RefCell::new(None),
            presets_path: RefCell::new(None),
            project_path: RefCell::new(None),
            config_path: RefCell::new(None),
            io_config_path: RefCell::new(None),
            selection_state: Arc::new(RwLock::new(SelectionState::default())),
            di_loop_state: RefCell::new(HashMap::new()),
            engine_sr: RefCell::new(REFERENCE_SAMPLE_RATE),
            async_done_tx,
            async_done_rx,
            tone_doctor_runs: RefCell::new(HashMap::new()),
            tone_doctor_input: RefCell::new(None),
            io_bindings: RefCell::new(None),
            runtime_control: RefCell::new(None),
        }
    }

    /// #791: register how the doctor reaches this chain's live input. The
    /// adapter that owns the audio runtime (today `adapter-gui`) attaches it at
    /// startup; MCP and gRPC inherit it because the dispatcher is shared.
    pub fn attach_tone_doctor_input(&self, provider: ToneDoctorInput) {
        *self.tone_doctor_input.borrow_mut() = Some(provider);
    }

    /// #127: register the frontend's audio runtime so runtime-control commands
    /// apply their effect from here instead of from a UI callback. Idempotent
    /// — the frontend re-attaches whenever it rebuilds its runtime handle.
    pub fn attach_runtime_control(&self, control: Rc<dyn RuntimeControl>) {
        *self.runtime_control.borrow_mut() = Some(control);
    }

    /// The attached runtime control, cloned OUT of its `RefCell`.
    ///
    /// Always reach the runtime through this: the frontend's sync sequence
    /// re-attaches the control on its way out, so calling a method while the
    /// `RefCell` is still borrowed panics with `BorrowMutError`. Cloning one
    /// `Rc` is the whole cost of not having that landmine.
    pub(crate) fn runtime_control(&self) -> Option<Rc<dyn RuntimeControl>> {
        self.runtime_control.borrow().clone()
    }

    /// #127: share the frontend's per-machine I/O binding registry handle, so
    /// the binding commands mutate the SAME allocation the frontend renders
    /// from and re-installs on every runtime sync. Same pattern as
    /// [`Self::attach_rig`]. Idempotent.
    pub fn attach_io_bindings(&self, registry: Rc<RefCell<Vec<IoBinding>>>) {
        *self.io_bindings.borrow_mut() = Some(registry);
    }
}

// ── Per-feature handlers (file-per-feature; #436 dispatcher split) ──────────
// This file is the thin router only. Each `handle_*` the `dispatch` match
// calls lives in its own sibling module (declared in `lib.rs`):
//   local_dispatcher_block_param     · handle_block_param
//   local_dispatcher_block_lifecycle · handle_block_lifecycle
//   local_dispatcher_block_edit      · handle_block_edit
//   local_dispatcher_chain_crud      · handle_chain_crud
//   local_dispatcher_chain_order     · handle_chain_order
//   local_dispatcher_chain_save      · handle_chain_save
//   local_dispatcher_chain_io        · handle_chain_io_replace
//   local_dispatcher_project         · handle_project
//   local_dispatcher_rig             · handle_rig_nav / capture / rename
// Each adds an `impl LocalDispatcher` block; behaviour is byte-identical to
// the previous single-file form (arm bodies moved verbatim).
