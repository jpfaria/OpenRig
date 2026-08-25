//! #127: the write-side counterpart to [`crate::live_source::LiveSource`].
//!
//! `LiveSource` lets the core READ what only a frontend's audio runtime
//! knows. `RuntimeControl` is the other direction: it lets a command handler
//! APPLY a state change to that runtime, so "every state change is born a
//! `Command`" holds for runtime control too. Before this, the GUI dispatched
//! the command AND then poked `ProjectRuntimeController` itself — which meant
//! MCP/gRPC dispatched the same command and nothing happened to the audio.
//!
//! The frontend that hosts the runtime implements this trait and hands the
//! dispatcher a shared instance via
//! [`crate::dispatcher::CommandDispatcher::attach_runtime_control`]. A
//! transport that owns no audio attaches nothing and keeps the default
//! no-ops: the command still succeeds and still reports its event, it just
//! has no runtime to touch.
//!
//! It is handed over as an `Rc` rather than a `Box` on purpose: a handler
//! clones the handle out of the dispatcher's `RefCell` and drops the borrow
//! BEFORE calling into the frontend. The frontend's sync sequence re-attaches
//! the control on its way out (it is the same funnel that installs it), so a
//! borrow held across the call would panic with `BorrowMutError`.
//!
//! **Not an audio-thread interface.** Every method is called on the
//! dispatching (UI/control) thread and must only flip control-plane state the
//! audio callback reads lock-free — no allocation, lock, syscall or I/O may
//! be pushed onto the audio thread from here.
//!
//! **Isolation (`CLAUDE.md` LAW).** A method that addresses a single stream
//! must carry that stream's identity; grouping runtimes by sample rate, or by
//! "all that match", is forbidden. `set_output_muted` is deliberately
//! rig-wide — the tuner silences the whole rig — and says so, rather than
//! pretending to be per-stream.

use std::sync::Arc;

use anyhow::Result;

use domain::ids::{BlockId, ChainId};
use domain::io_binding::IoBinding;
use engine::{DiPcm, LoopPcm};
use feature_dsp::metronome::MetronomeSettings;
use project::chain::{Chain, EndpointRef};
use project::device::DeviceSettings;

use crate::command::{LooperAction, LooperParam};
use crate::looper_edit::LoopEdit;

/// Runtime state changes a command handler can apply to the frontend's audio
/// runtime.
///
/// Every method defaults to a no-op, meaning "this frontend hosts no audio
/// runtime". A frontend implements only the controls it can actually honour.
pub trait RuntimeControl {
    /// Silence (or un-silence) the rig's output.
    ///
    /// Rig-wide by design: this is the tuner's mute, which exists so the
    /// player can tune without the audience hearing it. It is NOT a
    /// per-stream control, and must not be repurposed as one — a per-stream
    /// mute would take the stream's identity as an argument.
    fn set_output_muted(&self, muted: bool) {
        let _ = muted;
    }

    /// Install the per-machine I/O binding registry the live runtime resolves
    /// its device endpoints against, so a rig that is ALREADY running picks up
    /// a binding edit instead of waiting for the next cold start.
    fn set_io_bindings(&self, bindings: Vec<IoBinding>) {
        let _ = bindings;
    }

    /// Stop the rig: drop every stream this frontend has open and report the
    /// engine rate as "nothing running" again.
    ///
    /// Rig-wide by design, like [`Self::set_output_muted`], and it says so:
    /// this is "the rig stops", not "one stream stops". Tearing ONE stream
    /// down is [`Self::remove_chain`], which takes that stream's identity.
    ///
    /// Idempotent and never an error: there is nothing to fail about silence.
    fn stop_project_runtime(&self) {}

    /// Make the MACHINE's devices adopt these settings — the step that has to
    /// happen before the graph is re-opened against them.
    ///
    /// On macOS/Windows the implementation builds a throwaway stream at the
    /// requested rate, which is what forces CoreAudio / WASAPI to reconfigure
    /// the device; on Linux+JACK the server owns the device configuration and
    /// this is a no-op. It was the settings screen's own call, made right
    /// before dispatching [`crate::command::SettingsCommand::SaveAudioSettings`]
    /// — so the same command over MCP/gRPC persisted the pick, re-opened the
    /// graph, and left the driver on its old rate.
    ///
    /// Machine-wide by design, and it says so rather than pretending to be
    /// per-stream: these are the devices the PROJECT names, configured one at a
    /// time by their own id. It changes no project state — only what this
    /// machine's hardware is set to — and it never opens a chain's stream.
    ///
    /// Infallible on purpose: a device that answers slowly (USB interfaces
    /// routinely time out and then apply the change anyway) must not fail the
    /// save. An implementation logs and carries on, exactly as the GUI did.
    fn apply_device_settings(&self, settings: &[DeviceSettings]) {
        let _ = settings;
    }

    /// Rebuild the WHOLE running graph from the project as it stands now.
    ///
    /// This exists for the one change no per-chain sync can express: the
    /// device settings (sample rate, buffer size, bit depth, and on
    /// Linux/JACK the server parameters) apply to every device the project
    /// names at once, so every chain's stream has to be re-opened against them.
    ///
    /// **Isolation (`CLAUDE.md` LAW).** "Whole project" is an explicit list of
    /// chain identities — the chains the PROJECT names — walked one at a time,
    /// each against its own resolved devices. It must never become a selection
    /// over live runtimes by sample rate, or by "every runtime that matches":
    /// that is the rate-grouping this repo forbids, and the reason this door is
    /// spelled "sync the project" and not "sync everything at rate R".
    ///
    /// Never starts audio: with no controller there is nothing to rebuild, and
    /// a device-settings save on a stopped rig must leave it stopped.
    fn sync_project(&self) -> Result<()> {
        Ok(())
    }

    /// Drop ONE chain from the live graph — the chain-delete teardown.
    ///
    /// **This is not [`Self::sync_chain`] for a chain that is gone.** The two
    /// look interchangeable and are not: a sync validates the whole project
    /// first, so an unrelated invalid chain would make the delete's teardown
    /// fail and leave a deleted chain still sounding. This door removes the
    /// stream the command already removed from the project, and nothing else.
    ///
    /// **Isolation:** exactly the chain named by `chain`. No other stream's
    /// runtime may be stopped, rebuilt or observed — a delete is the operation
    /// where "it also tore down the neighbours" is most tempting and most
    /// wrong. The frontend drops its controller only when NOTHING is left
    /// sounding (no chain, no pending activation, no armed DI — #808).
    ///
    /// Idempotent and never an error, like [`Self::disarm_di_stream`].
    fn remove_chain(&self, chain: &ChainId) {
        let _ = chain;
    }

    /// Apply one block's new enabled state to the chain that owns it, LIVE.
    ///
    /// This is issue #522's in-place fade toggle: the frontend flips the
    /// block's state on the running graph without re-resolving devices and
    /// without rebuilding the stream, so a stomp on a pedal never drops audio.
    /// An implementation must NOT turn this into a stream restart.
    ///
    /// **Isolation:** `chain` names the one runtime to touch. No other
    /// stream's runtime may be paused, rebuilt or otherwise observed — and
    /// never select runtimes by sample rate (`CLAUDE.md` LAW).
    fn set_block_enabled(&self, chain: &ChainId, block: &BlockId, enabled: bool) -> Result<()> {
        let _ = (chain, block, enabled);
        Ok(())
    }

    /// Bring ONE chain's live runtime back in step with the project — start it
    /// if it is enabled and nothing is running, pause it when it is disabled,
    /// drop it when it is gone.
    ///
    /// The sequence itself (device resolve, off-thread rebuild, activation
    /// scheduling) belongs to the frontend that owns the runtime; this is the
    /// door a `Command` knocks on so no UI callback has to reach for the
    /// controller directly.
    ///
    /// **Isolation:** exactly the chain named by `chain`. Syncing one chain
    /// must not touch, group with, or depend on any other stream's runtime.
    fn sync_chain(&self, chain: &ChainId) -> Result<()> {
        let _ = chain;
        Ok(())
    }

    /// Start playing `pcm` as THIS chain's virtual DI (#614/#771).
    ///
    /// The DI is an independent pipeline (invariant #4): it renders through a
    /// copy of the chain's block graph onto the chain's own chosen output and
    /// never touches the guitar runtime. `chain` is the whole (live) chain
    /// because the arm resolves that chain's `di_output` and blocks; `pcm` is
    /// the decoded, still un-resampled source — the implementation resamples
    /// to the output's rate, off the audio thread.
    ///
    /// **Isolation:** exactly the stream named by `chain.id`. Never a group,
    /// never "every chain at this rate" (`CLAUDE.md` LAW).
    ///
    /// **#808:** the DI must play with NO chain enabled, so an implementation
    /// may create its audio runtime here if it has none. That is a
    /// precondition of the operation the user asked for — it is not a licence
    /// for any other door to wake audio up (see [`Self::sync_chain`], which
    /// must never start a runtime).
    fn arm_di_stream(&self, chain: &Chain, pcm: Arc<DiPcm>) -> Result<()> {
        let _ = (chain, pcm);
        Ok(())
    }

    /// Stop this chain's virtual DI. Idempotent, and never an error: there is
    /// nothing to fail about silence.
    ///
    /// **Isolation:** exactly the stream named by `chain`.
    fn disarm_di_stream(&self, chain: &ChainId) {
        let _ = chain;
    }

    /// Re-resolve a PLAYING DI stream after something it depends on changed —
    /// the chosen output endpoint (#771), or the device rate the loop was
    /// resampled to (#669).
    ///
    /// Distinct from [`Self::arm_di_stream`] on purpose: a stream that is NOT
    /// playing must stay silent, and this must never create a runtime. It is
    /// "follow the change if you are already sounding", not "start".
    fn refresh_di_stream(&self, chain: &Chain, pcm: Arc<DiPcm>) -> Result<()> {
        let _ = (chain, pcm);
        Ok(())
    }

    /// Start the metronome's click on the output endpoint `output_key` names
    /// (#14). `None` means "the project's first endpoint" — the same fallback
    /// a renamed binding or a different machine falls back to.
    ///
    /// The key is opaque here on purpose: `application` must never learn about
    /// devices or channels (see [`crate::live_source`]). The frontend that
    /// owns the audio host resolves the key against ITS binding registry.
    ///
    /// **Isolation:** the click is an independent pipeline (invariant #4). It
    /// opens its OWN output stream and must never be mixed into, routed
    /// through, or grouped with any chain's runtime — the backend sums it.
    ///
    /// **#808:** like [`Self::arm_di_stream`], the click must sound with NO
    /// chain enabled, so an implementation may create its audio runtime here.
    /// That licence belongs to this door alone; the three below must never
    /// wake audio up.
    fn start_metronome(&self, settings: MetronomeSettings, output_key: Option<&str>) -> Result<()> {
        let _ = (settings, output_key);
        Ok(())
    }

    /// Stop the click and close its stream. Idempotent, and never an error:
    /// there is nothing to fail about silence.
    fn stop_metronome(&self) {}

    /// Hand new settings to a click that may be running. Cheap and
    /// idempotent — the shared cell bumps a generation counter and the audio
    /// callback only re-reads when it changed, so a tempo edit never restarts
    /// the stream and never drops audio.
    ///
    /// Never starts anything: a settings edit is not a play.
    fn set_metronome_settings(&self, settings: MetronomeSettings) {
        let _ = settings;
    }

    /// Move a PLAYING click to the endpoint `output_key` now names.
    ///
    /// Distinct from [`Self::start_metronome`] for the same reason
    /// [`Self::refresh_di_stream`] is distinct from the arm: a click that is
    /// not sounding must stay silent, and this must never create a runtime.
    fn refresh_metronome_output(&self, output_key: Option<&str>) -> Result<()> {
        let _ = output_key;
        Ok(())
    }

    // ── analyzers (#544/#546/#829) ──────────────────────────────────────
    //
    // The tuner and the spectrum are OBSERVATION pipelines: each subscribes to
    // the taps of every enabled chain's inputs (invariant #4 — one
    // subscription per stream identity, never a group), runs its detection
    // next to the audio and publishes the RESULT through
    // `LiveSource::tuner` / `LiveSource::spectrum`.
    //
    // Powering one on is what makes it subscribe and start reading, and before
    // this it happened in the GUI's POWER callback — so `SetTunerEnabled` from
    // the MIDI footswitch (`adapter-midi`'s `toggle_tuner` slot) or from an MCP
    // client flipped the mirror in `SelectionState`, reported its event, and no
    // analyzer ever started. `openrig://tuner` then answered `running: false`
    // with no rows, telling the client to dispatch the very command that had
    // just done nothing.
    //
    // Neither door starts audio (#808): an analyzer reads what is already
    // sounding, and asking to look at a stopped rig is not asking to hear
    // anything. With nothing hosted it simply subscribes to nothing and
    // publishes no rows.

    /// Power the tuner's analyzer on or off.
    ///
    /// Infallible, both ways: subscribing is best-effort (a stream that is not
    /// there yields no row, never an error) and stopping is a teardown — there
    /// is nothing to fail about not looking.
    fn set_tuner_running(&self, running: bool) {
        let _ = running;
    }

    /// Power the spectrum's analyzer on or off. Same contract as
    /// [`Self::set_tuner_running`].
    fn set_spectrum_running(&self, running: bool) {
        let _ = running;
    }

    // ── loopers (#323) ──────────────────────────────────────────────────
    //
    // A loop lives in the frontend's looper store, not in the project: the
    // project only remembers that a looper EXISTS and what its knobs are set
    // to. So every command below has a project half the dispatcher owns and a
    // runtime half only these doors can reach — and before #127 the runtime
    // half was applied by the GUI callback that had just dispatched, which is
    // why a looper driven over MCP/MIDI recorded nothing.
    //
    // Every door takes the whole (post-mutation) `chain`, for the same reason
    // `arm_di_stream` does: the loop's isolated playback stream is reconciled
    // against THAT chain's looper list and its chosen endpoints, so the door
    // cannot address a stream it was not handed. Never a group, never "every
    // chain at this rate" (`CLAUDE.md` LAW).

    /// Claim this chain's store slot for a newly added looper.
    ///
    /// **#808:** a looper the user cannot record into is not a looper — the
    /// panel arms REC only against a live store — so this door may create its
    /// audio runtime, exactly like [`Self::arm_di_stream`] may. The other
    /// looper doors below must never wake audio.
    fn create_looper(&self, chain: &Chain, looper: u64) -> Result<()> {
        let _ = (chain, looper);
        Ok(())
    }

    /// Free the store slot and silence the loop's stream. Idempotent, and
    /// never an error: there is nothing to fail about deleting silence.
    fn remove_looper(&self, chain: &Chain, looper: u64) {
        let _ = (chain, looper);
    }

    /// Apply one transport action — record, stop, play, undo, redo, clear —
    /// to this looper, then reconcile its playback stream.
    ///
    /// `PlayStop` is resolved HERE, against the store's current state, so the
    /// one-button footswitch and the on-screen button behave identically.
    ///
    /// **#808:** `Record`, `Play` and `PlayStop` may create the audio runtime
    /// (a transport start is a request to hear something). `Stop`, `Clear`,
    /// `Undo` and `Redo` must not — silencing is never a reason to open a
    /// device.
    fn looper_transport(&self, chain: &Chain, looper: u64, action: LooperAction) -> Result<()> {
        let _ = (chain, looper, action);
        Ok(())
    }

    /// Push a mix / decay / speed / reverse edit to the live loop. Never
    /// starts anything: turning a knob is not a play.
    fn set_looper_param(&self, chain: &Chain, looper: u64, param: LooperParam) {
        let _ = (chain, looper, param);
    }

    /// Record from the chosen input endpoint from the next REC on (drops the
    /// current record tap so it re-subscribes). Never starts anything.
    fn set_looper_input(&self, chain: &Chain, looper: u64, input: Option<EndpointRef>) {
        let _ = (chain, looper, input);
    }

    /// Play to the chosen output endpoint. Never starts anything.
    fn set_looper_output(&self, chain: &Chain, looper: u64, output: Option<EndpointRef>) {
        let _ = (chain, looper, output);
    }

    /// #826: reshape a STOPPED loop — trim / crop / cut — and install the
    /// result, returning its new length in frames.
    ///
    /// Never wakes audio: an edit is not a request to hear something (#808).
    /// The store refuses the edit while the looper is recording or playing, so
    /// a live take is never reshaped under the player's feet.
    fn apply_looper_edit(&self, chain: &Chain, looper: u64, edit: LoopEdit) -> Result<usize> {
        let _ = (chain, looper, edit);
        Ok(0)
    }

    /// #826: step back one waveform edit. Independent of the transport's undo,
    /// which is a no-op for the single-take looper.
    fn undo_looper_edit(&self, chain: &Chain, looper: u64) {
        let _ = (chain, looper);
    }

    /// #826: step forward one undone waveform edit.
    fn redo_looper_edit(&self, chain: &Chain, looper: u64) {
        let _ = (chain, looper);
    }

    /// Hand over every recorded loop this chain holds, so the project save can
    /// write the sidecar wavs.
    ///
    /// **PCM travels as a HANDLE.** Each entry is an `Arc` of the mixdown the
    /// store already owns, carrying the rate it was recorded at — the samples
    /// are never copied across this seam, and they never travel through
    /// [`crate::live_source::LiveSource`], which returns finished readings
    /// only.
    ///
    /// Tri-state, like [`crate::live_source::LiveSource::devices`]: `None` ⇒
    /// no looper store is hosted right now (the rig is stopped), and the
    /// caller must leave every saved pointer alone — "nothing to export" is
    /// NOT "nothing was ever recorded", and collapsing the two erases the
    /// user's loops from the project. `Some(entries)` ⇒ hosted; a looper
    /// absent from `entries` holds no material and its stale pointer should be
    /// forgotten.
    fn export_chain_loops(&self, chain: &Chain) -> Option<Vec<(u64, Arc<LoopPcm>)>> {
        let _ = chain;
        None
    }

    // ── the frontend's own tick (#127, Task 15) ─────────────────────────
    //
    // The two doors below are the exception to "a `RuntimeControl` method is
    // applied by a command handler": nobody asks for either of them, so
    // neither is a `Command` (the report for Task 15 argues each). They are
    // here because they are still WRITES to the audio runtime, and the module
    // that performs them — the GUI's poll tick — must reach the runtime
    // through this seam like everything else, rather than holding the backend.

    /// #127/#323: bring ONE chain's looper store back in line with its
    /// config, feed whatever is recording, resolve each loop's LINKED preset
    /// into the blocks it plays through, and reconcile its isolated playback
    /// streams — in that order, or a Playing loop renders through the chain's
    /// current preset instead of its own.
    ///
    /// A WRITE, and — like the two below — deliberately NOT a `Command`: the
    /// frontend's meter tick runs it, and a tick is nobody's request. It is
    /// what makes a RECORD started from any transport actually capture, so the
    /// frontend that hosts the audio must keep ticking for the loops to move.
    ///
    /// Per chain, never "every chain": the slots, the recording drain and the
    /// playback streams all belong to this chain alone.
    fn reconcile_chain_loopers(&self, chain: &Chain) {
        let _ = chain;
    }

    /// #672: install any chain rebuild the control worker has finished, and
    /// return how many landed this tick.
    ///
    /// The heavy build runs off-thread and its result waits in a queue until
    /// this swaps it into the live slot, so a frontend that never calls this
    /// keeps playing the pre-edit graph.
    ///
    /// **Isolation:** every queued build carries the (chain, group) key it was
    /// requested for and is published into THAT slot alone. This services the
    /// queue; it never selects, groups or matches runtimes itself — and never
    /// by rate (`CLAUDE.md` LAW).
    ///
    /// **Never starts anything.** With no runtime there is no queue, so this
    /// is a no-op — a tick is not a request to hear something.
    fn apply_finished_rebuilds(&self) -> usize {
        0
    }

    /// Try to bring the audio backend back after [`LiveSource::audio_health`]
    /// reported it unhealthy: tear the streams down and re-open them for the
    /// project that is loaded.
    ///
    /// [`LiveSource::audio_health`]: crate::live_source::LiveSource::audio_health
    ///
    /// `Ok(true)` ⇒ recovered; `Ok(false)` ⇒ the hardware is still absent, so
    /// the caller should keep retrying; `Err` ⇒ the backend is back but the
    /// project failed to re-sync, which the caller must surface rather than
    /// retry blindly.
    ///
    /// Machine-wide by design, and it says so rather than pretending to be
    /// per-stream: the thing that died is the host every stream is open on
    /// (the JACK server), so recovery re-opens the project's streams together.
    /// It changes no project state — only which devices this machine holds.
    fn reconnect_audio(&self) -> Result<bool> {
        Ok(false)
    }
}
