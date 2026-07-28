use super::*;
use serde_json::Value;

#[test]
fn di_state_json_reports_playback_level_and_source_per_chain() {
    let rows = vec![
        DiLoopReading {
            chain: "rig:input-1".to_string(),
            playing: true,
            in_dbfs: -12.5,
            out_dbfs: -9.0,
            source: Some(DiLoopSource::Bundled("funk-90".to_string())),
        },
        DiLoopReading {
            chain: "rig:input-2".to_string(),
            playing: false,
            in_dbfs: -120.0,
            out_dbfs: -120.0,
            source: None,
        },
    ];

    let json: Value = serde_json::from_str(&di_loop_state_json(&rows)).unwrap();

    assert_eq!(json["chains"][0]["chain"], "rig:input-1");
    assert_eq!(json["chains"][0]["playing"], true);
    assert_eq!(json["chains"][0]["out_dbfs"], -9.0);
    assert_eq!(json["chains"][0]["source"]["Bundled"], "funk-90");
    assert_eq!(json["chains"][1]["playing"], false);
    assert!(json["chains"][1]["source"].is_null());
}
