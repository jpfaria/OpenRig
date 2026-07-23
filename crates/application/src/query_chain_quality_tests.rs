//! Tests for the objective chain-quality report query.

use std::sync::Once;

use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

use super::chain_quality_report;

fn init_registry() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        engine::native_registry::register_all_natives();
    });
}

fn clean_chain(id: &str) -> Chain {
    let schema = schema_for_block_model("gain", "volume").unwrap();
    let mut ps = ParameterSet::default();
    ps.insert("volume", ParameterValue::Float(80.0));
    let ps = ps.normalized_against(&schema).unwrap();
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![AudioBlock {
            id: BlockId("vol".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: ps,
            }),
        }],
        di_output: None,
        loopers: vec![],
    }
}

fn project(chains: Vec<Chain>) -> Project {
    Project {
        name: Some("q".into()),
        device_settings: vec![],
        chains,
        midi: None,
    }
}

#[test]
fn reports_metrics_json_for_a_clean_chain() {
    init_registry();
    let p = project(vec![clean_chain("c1")]);
    let out = chain_quality_report(&p, &ChainId("c1".into())).expect("report");

    let v: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let q = &v["quality"];
    assert!(q.is_object(), "envelope has a quality object: {out}");
    assert!(q["thd_n"].as_f64().unwrap() < 0.05, "clean chain low THD+N: {out}");
    assert!(q["noise_floor_dbfs"].as_f64().unwrap() <= -100.0, "low noise floor: {out}");
    assert_eq!(q["clip_fraction"].as_f64().unwrap(), 0.0, "no clipping: {out}");
}

#[test]
fn unknown_chain_is_an_error() {
    let p = project(vec![]);
    assert!(chain_quality_report(&p, &ChainId("missing".into())).is_err());
}
