use super::*;

// --- compute_eq_curves: sample-rate dependence (#723) ---

#[test]
fn compute_eq_curves_reflects_the_device_sample_rate() {
    use domain::value_objects::ParameterValue;
    use project::param::ParameterSet;
    // Boost the high shelf so the near-Nyquist response — where bilinear
    // warping differs most between rates — is non-flat.
    let mut params = ParameterSet::default();
    params.insert("high_gain", ParameterValue::Float(12.0));

    let at_44100 = compute_eq_curves("filter", "eq_three_band_basic", &params, 44_100.0);
    let at_96000 = compute_eq_curves("filter", "eq_three_band_basic", &params, 96_000.0);

    assert!(!at_44100.1.is_empty(), "model must yield EQ band curves");
    assert_ne!(
        at_44100, at_96000,
        "EQ curve must be drawn at the real device rate (issue #723), not a hardcoded 48k"
    );
}

// --- db_to_linear / linear_to_db ---

#[test]
fn db_to_linear_zero_db_is_unity() {
    let result = db_to_linear(0.0);
    assert!((result - 1.0).abs() < 1e-6);
}

#[test]
fn db_to_linear_minus_20_is_point_one() {
    let result = db_to_linear(-20.0);
    assert!((result - 0.1).abs() < 1e-6);
}

#[test]
fn db_to_linear_plus_20_is_ten() {
    let result = db_to_linear(20.0);
    assert!((result - 10.0).abs() < 1e-4);
}

#[test]
fn linear_to_db_roundtrip() {
    for db in [-12.0_f32, -6.0, 0.0, 3.0, 6.0, 12.0] {
        let lin = db_to_linear(db);
        let back = linear_to_db(lin);
        assert!(
            (back - db).abs() < 1e-4,
            "roundtrip failed for {} dB: got {}",
            db,
            back
        );
    }
}

// --- freq_to_x / gain_to_y ---

#[test]
fn freq_to_x_min_freq_returns_zero() {
    let x = freq_to_x(20.0);
    assert_eq!(x, 0.0);
}

#[test]
fn freq_to_x_max_freq_returns_svg_width() {
    let x = freq_to_x(20_000.0);
    assert_eq!(x, 1000.0);
}

#[test]
fn gain_to_y_zero_db_returns_mid_height() {
    let y = gain_to_y(0.0);
    assert_eq!(y, 100.0); // EQ_SVG_H / 2
}

#[test]
fn gain_to_y_max_gain_returns_zero() {
    let y = gain_to_y(24.0);
    assert_eq!(y, 0.0);
}

#[test]
fn gain_to_y_min_gain_returns_svg_height() {
    let y = gain_to_y(-24.0);
    assert_eq!(y, 200.0);
}

// --- biquad_kind_for_group ---

#[test]
fn biquad_kind_low_group_returns_low_shelf() {
    assert!(matches!(
        biquad_kind_for_group("Low Band"),
        block_core::BiquadKind::LowShelf
    ));
    assert!(matches!(
        biquad_kind_for_group("low"),
        block_core::BiquadKind::LowShelf
    ));
}

#[test]
fn biquad_kind_high_group_returns_high_shelf() {
    assert!(matches!(
        biquad_kind_for_group("High Band"),
        block_core::BiquadKind::HighShelf
    ));
    assert!(matches!(
        biquad_kind_for_group("HIGH"),
        block_core::BiquadKind::HighShelf
    ));
}

#[test]
fn biquad_kind_mid_group_returns_peak() {
    assert!(matches!(
        biquad_kind_for_group("Mid"),
        block_core::BiquadKind::Peak
    ));
    assert!(matches!(
        biquad_kind_for_group(""),
        block_core::BiquadKind::Peak
    ));
}

// --- eq_frequencies ---

#[test]
fn eq_frequencies_returns_200_points() {
    let freqs = eq_frequencies();
    assert_eq!(freqs.len(), 200);
}

#[test]
fn eq_frequencies_starts_at_20_hz_ends_at_20k_hz() {
    let freqs = eq_frequencies();
    assert!((freqs[0] - 20.0).abs() < 0.1);
    assert!((freqs[199] - 20_000.0).abs() < 1.0);
}

#[test]
fn eq_frequencies_monotonically_increasing() {
    let freqs = eq_frequencies();
    for i in 1..freqs.len() {
        assert!(
            freqs[i] > freqs[i - 1],
            "freq[{}]={} must be > freq[{}]={}",
            i,
            freqs[i],
            i - 1,
            freqs[i - 1]
        );
    }
}

// --- db_vec_to_svg_path ---

#[test]
fn db_vec_to_svg_path_starts_with_move_command() {
    let dbs = vec![0.0; 200];
    let path = db_vec_to_svg_path(&dbs);
    assert!(
        path.starts_with("M "),
        "SVG path should start with M: {}",
        &path[..20]
    );
}

#[test]
fn db_vec_to_svg_path_contains_line_commands() {
    let dbs = vec![0.0; 200];
    let path = db_vec_to_svg_path(&dbs);
    assert!(path.contains(" L "), "SVG path should contain L commands");
}

#[test]
fn db_vec_to_svg_path_empty_dbs_returns_empty() {
    let path = db_vec_to_svg_path(&[]);
    assert!(path.is_empty());
}

// --- #127: the curve stops following a stream that stopped -----------------
//
// The rate now comes from the dispatcher (`engine_sr`), which the runtime
// lifecycle keeps in lock-step with the live device. That exposed a staleness
// the controller read hid: `engine_sr` was only ever pushed FORWARD, never put
// back, so after a 44.1 kHz rig was torn down the block editor kept drawing at
// 44 100 — a rate nothing was running at. With no stream, the honest rate is
// the reference (issue #723's sanctioned no-device fallback).

/// The state the lifecycle leaves behind once a rig has come up on a device
/// that negotiated 44.1 kHz: the dispatcher's engine rate IS 44 100.
fn session_after_a_44100_rig(
    chains: Vec<project::chain::Chain>,
) -> Rc<RefCell<Option<ProjectSession>>> {
    let session = ProjectSession::new(
        project::project::Project {
            name: None,
            device_settings: Vec::new(),
            chains,
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-eq-viz-rate-tests"),
    );
    session.dispatcher.attach_engine_sr(44_100);
    Rc::new(RefCell::new(Some(session)))
}

fn disabled_chain(id: &domain::ids::ChainId) -> project::chain::Chain {
    project::chain::Chain {
        id: id.clone(),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

fn high_shelf_boost() -> project::param::ParameterSet {
    use domain::value_objects::ParameterValue;
    let mut params = project::param::ParameterSet::default();
    // Near-Nyquist bilinear warping is what differs between rates, so the
    // curve only tells the rates apart with the high shelf boosted.
    params.insert("high_gain", ParameterValue::Float(12.0));
    params
}

fn curve_for(session: &Rc<RefCell<Option<ProjectSession>>>) -> (String, Vec<String>) {
    compute_eq_curves(
        "filter",
        "eq_three_band_basic",
        &high_shelf_boost(),
        eq_viz_sample_rate(session),
    )
}

/// Disabling the last chain is the teardown a user hits with the block editor
/// open: `sync_live_chain_runtime` drops the controller and then re-syncs the
/// engine rate. Driven through THAT function — not through the rate-sync
/// helper it delegates to — so the wiring is exercised, not assumed.
#[test]
fn disabling_the_last_chain_draws_the_curve_at_the_reference_rate() {
    let chain_id = domain::ids::ChainId("eq-viz-rate".into());
    let session = session_after_a_44100_rig(vec![disabled_chain(&chain_id)]);
    let while_running = curve_for(&session);
    assert_eq!(
        eq_viz_sample_rate(&session),
        44_100.0,
        "precondition: the curve follows the live 44.1 kHz stream"
    );

    // The chain was just disabled and the controller is gone — the state the
    // sync reaches after its own `if !runtime.is_running() { None }` teardown.
    let runtime: Rc<RefCell<Option<infra_cpal::ProjectRuntimeController>>> =
        Rc::new(RefCell::new(None));
    {
        let borrow = session.borrow();
        let open = borrow.as_ref().expect("session");
        crate::runtime_lifecycle::sync_live_chain_runtime(&runtime, open, &chain_id)
            .expect("a disabled chain with no runtime syncs cleanly");
    }

    let after_stop = curve_for(&session);
    assert_eq!(
        eq_viz_sample_rate(&session),
        EQ_VIZ_REFERENCE_SAMPLE_RATE,
        "with no stream running the curve must be drawn at the reference rate, \
         not at the rate of the device that just went away"
    );
    assert_ne!(
        after_stop, while_running,
        "the curve must actually change: still identical means it is still \
         being drawn at the stale 44.1 kHz"
    );
}

/// Deleting a chain is the THIRD teardown, and the one the first fix missed:
/// `remove_live_chain_runtime` drops that chain's streams, and when it was the
/// last one there is nothing left running — but the controller survived, so no
/// later sync ever fired and the curve kept the dead device's rate.
#[test]
fn deleting_the_last_chain_draws_the_curve_at_the_reference_rate() {
    let chain_id = domain::ids::ChainId("eq-viz-rate".into());
    // The delete wiring dispatches `RemoveChain` FIRST, so by the time
    // `remove_live_chain_runtime` runs the project no longer carries the chain.
    let session = session_after_a_44100_rig(vec![]);
    let while_running = curve_for(&session);
    assert_eq!(
        eq_viz_sample_rate(&session),
        44_100.0,
        "precondition: the curve follows the live 44.1 kHz stream"
    );

    let runtime: Rc<RefCell<Option<infra_cpal::ProjectRuntimeController>>> =
        Rc::new(RefCell::new(None));
    crate::runtime_lifecycle::remove_live_chain_runtime(&runtime, &session, &chain_id);

    let after_delete = curve_for(&session);
    assert_eq!(
        eq_viz_sample_rate(&session),
        EQ_VIZ_REFERENCE_SAMPLE_RATE,
        "deleting the last chain leaves nothing running, so the curve must be \
         drawn at the reference rate — the controller not being dropped by the \
         removal is exactly what kept the dead device's rate alive"
    );
    assert_ne!(
        after_delete, while_running,
        "the curve must actually change: still identical means it is still \
         being drawn at the stale 44.1 kHz"
    );
}

/// Leaving the project (or opening another one) calls `stop_project_runtime`,
/// the explicit teardown. Same contract: the editor stops drawing at the rate
/// of a stream that no longer exists.
///
/// The controller cell is empty because starting a real one needs an audio
/// device; what the test drives is the state a stopped 44.1 kHz rig leaves —
/// a dispatcher still reporting 44 100 — through the call the UI actually
/// makes when it tears the rig down.
#[test]
fn stopping_the_project_runtime_draws_the_curve_at_the_reference_rate() {
    let session = session_after_a_44100_rig(vec![]);
    let while_running = curve_for(&session);
    assert_eq!(
        eq_viz_sample_rate(&session),
        44_100.0,
        "precondition: the curve follows the live 44.1 kHz stream"
    );

    let runtime: Rc<RefCell<Option<infra_cpal::ProjectRuntimeController>>> =
        Rc::new(RefCell::new(None));
    crate::stop_project_runtime(&runtime, &session);

    let after_stop = curve_for(&session);
    assert_eq!(
        eq_viz_sample_rate(&session),
        EQ_VIZ_REFERENCE_SAMPLE_RATE,
        "stopping the rig must put the engine rate back to the reference — the \
         block editor can stay open with audio stopped, and it would otherwise \
         keep drawing at the last device's 44.1 kHz"
    );
    assert_eq!(
        after_stop,
        compute_eq_curves(
            "filter",
            "eq_three_band_basic",
            &high_shelf_boost(),
            EQ_VIZ_REFERENCE_SAMPLE_RATE
        ),
        "the drawn curve must be the reference-rate curve"
    );
    assert_ne!(
        after_stop, while_running,
        "the curve must actually change: still identical means it is still \
         being drawn at the stale 44.1 kHz"
    );
}
