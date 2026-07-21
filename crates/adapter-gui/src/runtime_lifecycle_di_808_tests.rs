//! #808 — playing the DI must NOT require a chain to be enabled.
//!
//! The owner: "I open the project, don't touch the chain, hit play on the DI —
//! nothing. I toggle the chain on/off and then play works." Root cause: the
//! runtime controller is created ONLY when a chain is enabled
//! (`sync_live_chain_runtime`), so with nothing enabled the play was a silent
//! no-op — there was no controller to arm the DI on. The DI is an independent
//! pipeline (invariant #4); `ensure_runtime` creates the controller regardless
//! of any chain being enabled.

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::ChainId;
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;
use project::project::Project;

use super::ensure_runtime;
use crate::state::ProjectSession;

fn session_with_disabled_chain() -> ProjectSession {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId("di808-play".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: false, // opened the project, never enabled the chain
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![],
            di_output: None,
        }],
        midi: None,
    };
    ProjectSession::new(
        project,
        None,
        None,
        std::env::temp_dir().join("openrig-808-play-tests"),
    )
}

#[test]
fn playing_the_di_gets_a_runtime_without_enabling_a_chain() {
    let session = session_with_disabled_chain();
    let project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>> =
        Rc::new(RefCell::new(None));

    assert!(
        project_runtime.borrow().is_none(),
        "precondition: no chain enabled, so no controller exists yet"
    );

    ensure_runtime(&project_runtime, &session).expect("ensure_runtime must succeed");

    assert!(
        project_runtime.borrow().is_some(),
        "#808: hitting play on the DI must create the runtime controller even \
         though no chain is enabled — otherwise arming the DI is a silent no-op \
         until a chain toggle happens to create one."
    );
}

// ── #808: the GUI's live-sync path, on real hardware ───────────────────────
//
// The controller path (upsert_chain + re-arm) is proven on the rig by
// `infra-cpal/tests/issue_808_di_param_edit_keeps_playing.rs`. What the app
// actually calls on a param edit is `sync_live_chain_runtime`, which ALSO
// pushes the session's io_bindings into the controller and validates the
// project before the upsert. This drives that path against a real output
// device and asserts the two things the owner reports failing: the DI must not
// fall silent, and the edit must reach its tone.
//
// Gated by OPENRIG_HW_TESTS=1 (macOS release, idle machine).
#[cfg(all(target_os = "macos", not(debug_assertions)))]
mod hw {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use domain::ids::{BlockId, ChainId, DeviceId};
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    use domain::value_objects::ParameterValue;
    use infra_cpal::{list_output_device_descriptors, ProjectRuntimeController};
    use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
    use project::chain::Chain;
    use project::device::DeviceSettings;
    use project::param::ParameterSet;
    use project::project::Project;

    use crate::runtime_lifecycle::{ensure_runtime, sync_live_chain_runtime};
    use crate::state::ProjectSession;

    const CHAIN_ID: &str = "di808-gui";
    const SILENT: f32 = 1e-4;

    fn gain_params(volume_pct: f32) -> ParameterSet {
        let schema = schema_for_block_model("gain", "volume").expect("volume schema");
        let mut ps = ParameterSet::default();
        ps.insert("volume", ParameterValue::Float(volume_pct));
        ps.normalized_against(&schema).expect("normalize")
    }

    fn chain(volume_pct: f32) -> Chain {
        Chain {
            id: ChainId(CHAIN_ID.into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: false, // the owner never enables it — only the DI plays
            volume: 100.0,
            io_binding_ids: vec!["io".into()],
            blocks: vec![AudioBlock {
                id: BlockId("di808gui:gain".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "gain".into(),
                    model: "volume".into(),
                    params: gain_params(volume_pct),
                }),
            }],
            di_output: None,
        }
    }

    fn out_peak(rt: &Rc<RefCell<Option<ProjectRuntimeController>>>, cid: &ChainId) -> f32 {
        rt.borrow()
            .as_ref()
            .and_then(|c| c.di_playback_peaks(cid))
            .map(|(_, o)| o)
            .unwrap_or(0.0)
    }

    fn max_out_peak(
        rt: &Rc<RefCell<Option<ProjectRuntimeController>>>,
        cid: &ChainId,
        window: Duration,
    ) -> f32 {
        let deadline = Instant::now() + window;
        let mut max = 0.0f32;
        while Instant::now() < deadline {
            max = max.max(out_peak(rt, cid));
            std::thread::sleep(Duration::from_millis(20));
        }
        max
    }

    #[test]
    fn a_param_edit_through_the_gui_sync_keeps_the_di_playing() {
        if std::env::var_os("OPENRIG_HW_TESTS").is_none() {
            eprintln!(
                "SKIP a_param_edit_through_the_gui_sync_keeps_the_di_playing: \
                 real-hardware battery. Run with OPENRIG_HW_TESTS=1 on an idle machine."
            );
            return;
        }
        engine::native_registry::register_all_natives();

        let outputs = list_output_device_descriptors().expect("outputs");
        let out = outputs.first().expect("an output device");
        let bindings = vec![IoBinding {
            id: "io".into(),
            name: "IO".into(),
            inputs: vec![],
            outputs: vec![IoEndpoint {
                name: "out".into(),
                device_id: DeviceId(out.id.clone()),
                mode: ChannelMode::Stereo,
                channels: vec![0, 1],
            }],
        }];
        let project = Project {
            name: None,
            device_settings: vec![DeviceSettings {
                device_id: DeviceId(out.id.clone()),
                sample_rate: 48_000,
                buffer_size_frames: 256,
                bit_depth: 32,
            }],
            chains: vec![chain(100.0)],
            midi: None,
        };
        let session = ProjectSession::new(
            project,
            None,
            None,
            std::env::temp_dir().join("openrig-808-gui-hw"),
        );
        *session.io_bindings.borrow_mut() = bindings;

        let rt: Rc<RefCell<Option<ProjectRuntimeController>>> = Rc::new(RefCell::new(None));
        let cid = ChainId(CHAIN_ID.into());

        // The DI play button: create the runtime, then arm.
        ensure_runtime(&rt, &session).expect("ensure_runtime");
        let pcm = std::sync::Arc::new(engine::DiPcm::new(vec![0.4; 48_000 * 4], 48_000, 1));
        rt.borrow()
            .as_ref()
            .expect("controller")
            .arm_di_stream(&chain(100.0), pcm)
            .expect("arm DI");

        let deadline = Instant::now() + Duration::from_secs(20);
        while out_peak(&rt, &cid) <= SILENT && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            out_peak(&rt, &cid) > SILENT,
            "#808 precondition: the DI never reached the output with no chain enabled"
        );
        let before = max_out_peak(&rt, &cid, Duration::from_secs(2));

        // The owner's action, through the path the GUI really uses.
        let edited = chain(10.0);
        session.project.borrow_mut().chains[0] = edited.clone();
        sync_live_chain_runtime(&rt, &session, &cid).expect("gui live sync must not error");

        // (a) it must NEVER fall silent from here on.
        let deadline = Instant::now() + Duration::from_secs(6);
        while Instant::now() < deadline {
            let p = out_peak(&rt, &cid);
            assert!(
                p > SILENT,
                "#808: the DI STOPPED after the param edit through the GUI sync \
                 (out peak {p:.5} fell silent) — the owner's 'troquei o parâmetro \
                 e o DI parou de tocar'."
            );
            std::thread::sleep(Duration::from_millis(50));
        }

        // (b) and the edit must reach the DI's tone (100% -> 10% volume).
        let after = max_out_peak(&rt, &cid, Duration::from_secs(2));
        assert!(
            after < before * 0.5,
            "#808: the param edit never reached the DI through the GUI sync — \
             peak stayed at {after:.5} (was {before:.5}) after dropping the gain \
             100% -> 10%. The owner's 'mudo o parâmetro e o timbre não muda'."
        );
    }
}
