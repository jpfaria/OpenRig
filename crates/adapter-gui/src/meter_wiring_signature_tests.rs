//! Timer-signature + build_streams meter tests (issue #792 split from
//! meter_wiring_tests.rs).

use std::sync::Arc;

use engine::spsc::SpscRing;


// ── timer-shape signature: tracks engine runtime layout, not just
//     project-side bits. User-reported bug: toggle off → on, meters
//     never recover. Project state alone (`chain.enabled`) flips
//     correctly, but the timer's invalidation must ALSO catch the
//     engine tearing down + rebuilding the per-input runtimes —
//     otherwise the cached rings point at producers that no longer
//     push (the old runtime instance got dropped by remove_chain).

#[test]
fn timer_signature_flips_on_toggle_off_then_on_cycle() {
    use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use project::chain::Chain;
    use project::param::ParameterSet;
    let mut c = Chain {
        id: domain::ids::ChainId("c1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![AudioBlock {
            id: domain::ids::BlockId("b1".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: ParameterSet::default(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    };
    // Initial: enabled, runtime up with 3 streams.
    let s_initial = crate::meter_wiring::timer_chain_signature(&c, 3);
    // Toggle off: enabled flips to false, engine tears down → stream_count=0.
    c.enabled = false;
    let s_off = crate::meter_wiring::timer_chain_signature(&c, 0);
    assert_ne!(
        s_initial, s_off,
        "toggle off must flip the signature so the timer invalidates \
         the dead ring handles left by the torn-down runtime"
    );
    // Toggle on: enabled flips back to true, fresh runtime → stream_count=3.
    c.enabled = true;
    let s_on = crate::meter_wiring::timer_chain_signature(&c, 3);
    assert_ne!(
        s_off, s_on,
        "toggle on must flip the signature again so the timer drops \
         the empty cache entry and re-subscribes against the freshly \
         rebuilt per-input runtimes"
    );
}

#[test]
fn controller_offline_then_back_invalidates_every_chain() {
    // User-reported: toggle off the last enabled chain → meter dies
    // and never recovers on toggle on. Root cause: when the whole
    // chain set goes empty, `sync_live_chain_runtime` drops the
    // `ProjectRuntimeController`. The timer's early return on the
    // None controller leaves the per-chain signature cache intact,
    // so when the next toggle-on call instantiates a FRESH
    // controller, the same project state yields the same cached
    // signature → no invalidation fires → the store keeps handing
    // out rings allocated against the dropped controller (nothing
    // pushes to them). The fix is to wipe the cache whenever the
    // controller is offline so the next online tick treats every
    // chain as a fresh subscription.
    use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use project::chain::Chain;
    use project::param::ParameterSet;
    let chain = Chain {
        id: domain::ids::ChainId("c1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![AudioBlock {
            id: domain::ids::BlockId("b1".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: ParameterSet::default(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    };
    let mut last_sig: std::collections::HashMap<domain::ids::ChainId, u64> =
        std::collections::HashMap::new();
    let api_on = RecordingTapApi {
        stream_count: 3,
        ..Default::default()
    };
    // Tick 1: controller online with 3-stream chain.
    let inv1 = crate::meter_wiring::detect_invalidations(
        std::slice::from_ref(&chain),
        Some(&api_on),
        &mut last_sig,
    );
    assert_eq!(inv1.len(), 1, "first online tick subscribes the chain");
    assert!(last_sig.contains_key(&chain.id), "signature cached");
    // Tick 2: controller offline (last chain toggled off).
    let _ = crate::meter_wiring::detect_invalidations::<RecordingTapApi>(
        std::slice::from_ref(&chain),
        None,
        &mut last_sig,
    );
    assert!(
        last_sig.is_empty(),
        "offline tick must wipe the cache; otherwise a fresh \
         controller with identical project state would compare \
         equal against the stale entry and skip the re-subscribe"
    );
    // Tick 3: fresh controller comes back (different api instance,
    // same project state).
    let api_on2 = RecordingTapApi {
        stream_count: 3,
        ..Default::default()
    };
    let inv3 = crate::meter_wiring::detect_invalidations(
        std::slice::from_ref(&chain),
        Some(&api_on2),
        &mut last_sig,
    );
    assert_eq!(
        inv3.len(),
        1,
        "fresh controller must invalidate every chain so dead rings \
         get re-subscribed against the new per-input runtimes"
    );
}

#[test]
fn timer_signature_stays_constant_across_steady_state_ticks() {
    use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use project::chain::Chain;
    use project::param::ParameterSet;
    let c = Chain {
        id: domain::ids::ChainId("c1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![AudioBlock {
            id: domain::ids::BlockId("b1".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: ParameterSet::default(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    };
    // Two consecutive steady-state ticks: same enabled, same blocks,
    // same stream_count → signature stable, no re-subscribe (flicker
    // regression guard).
    let a = crate::meter_wiring::timer_chain_signature(&c, 3);
    let b = crate::meter_wiring::timer_chain_signature(&c, 3);
    assert_eq!(a, b, "steady-state ticks must NOT re-subscribe");
}

// ── build_streams_from_taps: per-entry meter routing.
// Original bug (3-source screenshot): input shows in slot 3, output
// only in slot 1. Pinned causes covered here historically:
//   (a) input was subscribed as channels [0..N] of runtime[0] instead
//       of one ring per per-input runtime (issue #350 layout);
//   (b) the chain stream_tap was subscribed ONCE (stream_index=0) and
//       its Arc cloned into every row — SPSC ring has a single
//       consumer, so row 0 drains it and rows 1..N see empty.
// Issue #557 closed the follow-up "stream 1 silent + wrong device
// channel" gap: the helper now drives a single high-level call —
// `subscribe_stream_input_tap(cid, i, cap)` — and the controller
// resolves runtime, cpal group, and endpoint channel inside.
// The new helper must:
//   - call subscribe_stream_input_tap once per global stream_index 0..N
//   - call subscribe_stream_tap once per global stream_index 0..N
//   - place each subscription in its own row (no Arc broadcasting).

#[derive(Default)]
struct RecordingTapApi {
    stream_count: usize,
    stream_input_calls: std::cell::RefCell<Vec<usize>>, // stream_index
    stream_calls: std::cell::RefCell<Vec<usize>>,       // stream_index
}

impl crate::meter_wiring::MeterTapApi for RecordingTapApi {
    fn stream_count(&self, _cid: &domain::ids::ChainId) -> usize {
        self.stream_count
    }
    fn subscribe_stream_input_tap(
        &self,
        _cid: &domain::ids::ChainId,
        stream_index: usize,
        _capacity: usize,
    ) -> Option<Arc<SpscRing<f32>>> {
        self.stream_input_calls.borrow_mut().push(stream_index);
        Some(Arc::new(SpscRing::<f32>::new(16, 0.0)))
    }
    fn subscribe_stream_tap(
        &self,
        _cid: &domain::ids::ChainId,
        stream_index: usize,
        _capacity: usize,
    ) -> Option<[Arc<SpscRing<f32>>; 2]> {
        self.stream_calls.borrow_mut().push(stream_index);
        Some([
            Arc::new(SpscRing::<f32>::new(16, 0.0)),
            Arc::new(SpscRing::<f32>::new(16, 0.0)),
        ])
    }
}

#[test]
fn build_streams_subscribes_stream_input_tap_once_per_global_index() {
    let api = RecordingTapApi {
        stream_count: 3,
        ..Default::default()
    };
    let cid = domain::ids::ChainId("c1".into());
    let _ = crate::meter_wiring::build_streams_from_taps(&api, &cid, 4096);
    assert_eq!(
        *api.stream_input_calls.borrow(),
        vec![0_usize, 1, 2],
        "input meter must subscribe via the per-stream API once per \
         global stream index; the controller resolves the runtime, \
         cpal group, and endpoint channel — issue #557"
    );
}

#[test]
fn build_streams_subscribes_one_stream_tap_per_global_index_not_just_zero() {
    let api = RecordingTapApi {
        stream_count: 3,
        ..Default::default()
    };
    let cid = domain::ids::ChainId("c1".into());
    let _ = crate::meter_wiring::build_streams_from_taps(&api, &cid, 4096);
    assert_eq!(
        *api.stream_calls.borrow(),
        vec![0_usize, 1, 2],
        "stream_tap must be subscribed once per GLOBAL stream index so \
         each row gets its own ring; sharing one ring across rows means \
         the first row drains the SPSC and rows 1..N stay silent"
    );
}

#[test]
fn build_streams_produces_one_row_per_stream_with_distinct_rings() {
    let api = RecordingTapApi {
        stream_count: 3,
        ..Default::default()
    };
    let cid = domain::ids::ChainId("c1".into());
    let chain = crate::meter_wiring::build_streams_from_taps(&api, &cid, 4096);
    assert_eq!(chain.streams.len(), 3, "one row per stream");
    // Every row must hold rings allocated by its OWN subscribe call —
    // not Arc::clone of the previous row's rings.
    let out_ptrs: Vec<*const SpscRing<f32>> = chain
        .streams
        .iter()
        .flat_map(|s| s.output.iter().map(Arc::as_ptr))
        .collect();
    let unique: std::collections::HashSet<_> = out_ptrs.iter().collect();
    assert_eq!(
        unique.len(),
        out_ptrs.len(),
        "output rings must be distinct across rows; sharing was the \
         bug — first row drained the SPSC, others saw silence"
    );
}

#[test]
fn build_streams_returns_empty_when_chain_has_no_runtime() {
    let api = RecordingTapApi {
        stream_count: 0,
        ..Default::default()
    };
    let cid = domain::ids::ChainId("c1".into());
    let chain = crate::meter_wiring::build_streams_from_taps(&api, &cid, 4096);
    assert!(
        chain.streams.is_empty(),
        "no runtime ⇒ no meter rows; UI shows the empty list and the \
         row count converges back the next tick when the runtime is \
         rebuilt"
    );
}

// ── Issue #670: per-chain audio-overload (xrun) indicator ────────────────

#[test]
fn chain_overloaded_when_new_xruns_since_last_poll() {
    // The audio callback counted more deadline misses than the previous
    // poll saw → the user is hearing dropouts right now.
    assert!(super::chain_overloaded(10, 13));
}

#[test]
fn chain_not_overloaded_when_xrun_count_is_stable() {
    assert!(!super::chain_overloaded(13, 13));
}

#[test]
fn chain_not_overloaded_when_counter_was_reset() {
    // reset_load_stats zeroed the counter between polls — not a new overrun.
    assert!(!super::chain_overloaded(13, 0));
}
