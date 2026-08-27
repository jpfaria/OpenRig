//! Probe (ignored by default): what an arm of the isolated playback costs.
//!
//! `cargo test -p engine --release --test issue_903_arm_cost_probe -- --ignored --nocapture`
use std::time::Instant;

#[test]
#[ignore = "probe: needs the machine's plugin catalog"]
fn build_cost_of_the_isolated_runtime() {
    let path = std::env::var("PROBE_CHAIN").expect("PROBE_CHAIN=<preset yaml>");
    let root = std::env::var("OPENRIG_PLUGINS_ROOT").expect("OPENRIG_PLUGINS_ROOT");
    engine::native_registry::register_all_natives();
    plugin_loader::registry::init_many(&[std::path::PathBuf::from(root)]);

    let preset =
        infra_yaml::load_chain_preset_file(std::path::Path::new(&path)).expect("read the preset");
    let chain = project::chain::Chain {
        id: domain::ids::ChainId("arm-cost-probe".into()),
        description: None,
        instrument: preset.instrument,
        enabled: true,
        volume: preset.volume,
        io_binding_ids: vec![],
        blocks: preset.blocks,
        di_output: None,
        loopers: vec![],
    };
    let pcm = engine::DiPcm::new(vec![0.3; 44_100 * 2], 44_100, 2);

    for round in 0..3 {
        let t = Instant::now();
        let routed = engine::di_render::build_routed_di_runtime(&chain, &[], None, 44_100, &pcm)
            .expect("build");
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        println!("arm #{round}: {ms:.1} ms");
        drop(routed);
    }
}
