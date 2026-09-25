//! #923 — the per-output-route read: what each route's device stream pulled.

use super::{output_routes_json, rows_for_chain, OutputRouteReading};
use engine::runtime_output_route_stats::OutputRouteStats;

#[test]
fn an_unhosted_frontend_answers_the_empty_shape() {
    let json = output_routes_json(false, &[]);
    assert_eq!(json, r#"{"hosted":false,"rows":[]}"#);
}

#[test]
fn a_hosted_row_carries_every_field_the_engine_counted() {
    let rows = vec![OutputRouteReading {
        chain: "rig:input-2".into(),
        group: 0,
        route: 1,
        channels: vec![16, 17],
        callbacks: 1234,
        underruns: 2,
        peak_dbfs: -23.5,
        fill_frames: 384,
        latency_trims: 1,
        dropped_frames: 640,
        input_busy_skips: 3,
    }];
    let json = output_routes_json(true, &rows);
    assert!(json.starts_with(r#"{"hosted":true,"rows":[{"#), "{json}");
    for field in [
        r#""chain":"rig:input-2""#,
        r#""group":0"#,
        r#""route":1"#,
        r#""channels":[16,17]"#,
        r#""callbacks":1234"#,
        r#""underruns":2"#,
        r#""peak_dbfs":-23.5"#,
        r#""fill_frames":384"#,
        r#""latency_trims":1"#,
        r#""dropped_frames":640"#,
        r#""input_busy_skips":3"#,
    ] {
        assert!(json.contains(field), "missing {field} in {json}");
    }
}

#[test]
fn rows_for_chain_flattens_every_group_and_route_in_order() {
    let stats = |route: usize, channels: Vec<usize>, callbacks: u64| OutputRouteStats {
        route,
        channels,
        callbacks,
        underruns: route as u64,
        peak_dbfs: -6.0 * route as f32,
        fill_frames: 100 + route,
        latency_trims: 2 * route as u64,
        dropped_frames: 0,
        input_busy_skips: 0,
    };
    let rows = rows_for_chain(
        "rig:input-2",
        vec![
            (
                0,
                vec![stats(0, vec![0, 1], 10), stats(1, vec![16, 17], 11)],
            ),
            (1, vec![stats(0, vec![0, 1], 20)]),
        ],
    );
    let keys: Vec<(String, usize, usize, Vec<usize>, u64, u64, f32)> = rows
        .into_iter()
        .map(|r| {
            (
                r.chain,
                r.group,
                r.route,
                r.channels,
                r.callbacks,
                r.underruns,
                r.peak_dbfs,
            )
        })
        .collect();
    assert_eq!(
        keys,
        vec![
            ("rig:input-2".into(), 0, 0, vec![0, 1], 10, 0, 0.0),
            ("rig:input-2".into(), 0, 1, vec![16, 17], 11, 1, -6.0),
            ("rig:input-2".into(), 1, 0, vec![0, 1], 20, 0, 0.0),
        ]
    );
}

#[test]
fn rows_for_chain_carries_each_route_cushion_and_its_trims() {
    let row = |route: usize| OutputRouteStats {
        route,
        channels: vec![0, 1],
        callbacks: 1,
        underruns: 0,
        peak_dbfs: 0.0,
        fill_frames: 300 + route,
        latency_trims: route as u64,
        dropped_frames: 0,
        input_busy_skips: 0,
    };
    let rows = rows_for_chain("rig:input-4", vec![(0, vec![row(0), row(1)])]);
    let got: Vec<(usize, u64)> = rows
        .iter()
        .map(|r| (r.fill_frames, r.latency_trims))
        .collect();
    assert_eq!(got, vec![(300, 0), (301, 1)]);
}

#[test]
fn rows_for_chain_with_no_runtime_is_empty() {
    assert!(rows_for_chain("rig:input-9", vec![]).is_empty());
}
