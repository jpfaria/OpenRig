//! #903 — the looper projections that write into a chain's on-screen row.
//!
//! These run offline: no device, no window callbacks. They pin what the panel
//! reads — the endpoint options a row offers, the preset bank behind the
//! drawer's picker, and the rows themselves with and without a live stream.

use crate::looper_rows::{
    apply_looper_endpoints_to_rows, apply_looper_presets_to_rows, chain_preset_ids,
    write_chain_looper_row,
};
use crate::project_view::replace_project_chains;
use crate::ProjectChainItem;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::{Chain, LooperConfig};
use project::project::Project;
use slint::{Model, VecModel};
use std::rc::Rc;

const BINDING: &str = "io-1";

fn binding() -> IoBinding {
    IoBinding {
        id: BINDING.into(),
        name: "Interface".into(),
        inputs: vec![IoEndpoint {
            name: "Guitar In".into(),
            device_id: DeviceId("dev-in".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "Main Out".into(),
            device_id: DeviceId("dev-out".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }
}

fn chain(loopers: Vec<LooperConfig>) -> Chain {
    Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec![BINDING.into()],
        blocks: vec![],
        di_output: None,
        loopers,
    }
}

fn project(loopers: Vec<LooperConfig>) -> Project {
    Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain(loopers)],
        midi: None,
    }
}

fn rows_for(project: &Project) -> Rc<VecModel<ProjectChainItem>> {
    let model = Rc::new(VecModel::<ProjectChainItem>::default());
    replace_project_chains(&model, project, &[], &[], &[]);
    model
}

fn strings(
    model: &Rc<VecModel<ProjectChainItem>>,
    pick: fn(&ProjectChainItem) -> Vec<String>,
) -> Vec<String> {
    pick(&model.row_data(0).expect("one row"))
}

#[test]
fn the_row_offers_the_chains_bound_endpoints_without_a_running_stream() {
    let project = project(vec![LooperConfig::new(1)]);
    let model = rows_for(&project);

    apply_looper_endpoints_to_rows(&model, &project, &[binding()]);

    let inputs = strings(&model, |r| {
        r.looper_input_options
            .iter()
            .map(|s| s.to_string())
            .collect()
    });
    let outputs = strings(&model, |r| {
        r.looper_output_options
            .iter()
            .map(|s| s.to_string())
            .collect()
    });
    assert!(
        inputs.iter().any(|s| s.contains("Guitar In")),
        "the record-from picker lists the chain's bound input — got {inputs:?}"
    );
    assert!(
        outputs.iter().any(|s| s.contains("Main Out")),
        "the play-to picker lists the chain's bound output — got {outputs:?}"
    );
}

#[test]
fn a_chain_with_no_binding_offers_no_endpoints() {
    let mut p = project(vec![LooperConfig::new(1)]);
    p.chains[0].io_binding_ids.clear();
    let model = rows_for(&p);

    apply_looper_endpoints_to_rows(&model, &p, &[binding()]);

    assert!(strings(&model, |r| r
        .looper_input_options
        .iter()
        .map(|s| s.to_string())
        .collect())
    .is_empty());
}

#[test]
fn a_non_rig_chain_has_an_empty_preset_bank() {
    let project = project(vec![LooperConfig::new(1)]);
    let model = rows_for(&project);

    apply_looper_presets_to_rows(&model, &project, None);

    assert!(strings(&model, |r| r
        .looper_preset_options
        .iter()
        .map(|s| s.to_string())
        .collect())
    .is_empty());
    assert!(chain_preset_ids(&project.chains[0], None).is_empty());
}

#[test]
fn the_rows_come_from_the_config_when_no_stream_is_running() {
    let project = project(vec![LooperConfig::new(1), LooperConfig::new(2)]);
    let model = rows_for(&project);

    write_chain_looper_row(
        &model,
        0,
        &project.chains[0],
        &[],
        None, // no live runtime
        &[binding()],
        &[],
    );

    let row = model.row_data(0).expect("one row");
    let items: Vec<crate::LooperItem> = row.loopers.iter().collect();
    assert_eq!(items.len(), 2, "both persisted loopers are on screen");
    assert!(
        !items[0].can_record,
        "REC is only armable against a live runtime"
    );
    assert!(!row.looper_active, "nothing is playing without a stream");
}

#[test]
fn a_live_rate_arms_record_on_the_rows() {
    let project = project(vec![LooperConfig::new(1)]);
    let model = rows_for(&project);

    write_chain_looper_row(
        &model,
        0,
        &project.chains[0],
        &[],
        Some(48_000),
        &[binding()],
        &[],
    );

    let items: Vec<crate::LooperItem> =
        model.row_data(0).expect("one row").loopers.iter().collect();
    assert!(
        items[0].can_record,
        "with a live runtime the panel may arm REC"
    );
}

#[test]
fn writing_a_row_index_that_does_not_exist_is_a_no_op() {
    let project = project(vec![LooperConfig::new(1)]);
    let model = rows_for(&project);

    write_chain_looper_row(&model, 7, &project.chains[0], &[], None, &[binding()], &[]);

    assert_eq!(model.row_count(), 1, "no row is invented for a bad index");
}
