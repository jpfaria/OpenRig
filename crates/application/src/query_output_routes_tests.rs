//! #923 — the per-output-route read: what each route's device stream pulled.

use super::{output_routes_json, OutputRouteReading};

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
    ] {
        assert!(json.contains(field), "missing {field} in {json}");
    }
}
