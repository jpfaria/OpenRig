// #1007: the IN/OUT meter section of a chain card is collapsible and starts
// collapsed. The chain list rebuilds its rows on almost every edit
// (`replace_project_chains`), so an expanded section must survive that rebuild
// for the same chain, and must not leak onto a different chain.

use crate::project_view::replace_project_chains;
use crate::ProjectChainItem;
use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;
use slint::{Model, VecModel};
use std::rc::Rc;

fn chain(name: &str) -> Chain {
    Chain {
        id: ChainId(format!("test:{name}")),
        description: Some(name.to_string()),
        instrument: "electric_guitar".to_string(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        mix: Default::default(),
    }
}

fn project(chains: Vec<Chain>) -> Project {
    Project {
        name: None,
        device_settings: Vec::new(),
        chains,
        midi: None,
    }
}

fn expand_row(model: &VecModel<ProjectChainItem>, row: usize) {
    let mut item = model.row_data(row).expect("row");
    item.meters_expanded = true;
    model.set_row_data(row, item);
}

fn expanded(model: &VecModel<ProjectChainItem>) -> Vec<bool> {
    model.iter().map(|r| r.meters_expanded).collect()
}

#[test]
fn a_chain_card_opens_with_its_meters_collapsed() {
    let model = Rc::new(VecModel::<ProjectChainItem>::default());
    replace_project_chains(
        &model,
        &project(vec![chain("A"), chain("B")]),
        &[],
        &[],
        &[],
    );
    assert_eq!(expanded(&model), vec![false, false]);
}

#[test]
fn an_expanded_section_survives_the_row_rebuild() {
    let model = Rc::new(VecModel::<ProjectChainItem>::default());
    let p = project(vec![chain("A"), chain("B")]);
    replace_project_chains(&model, &p, &[], &[], &[]);
    expand_row(&model, 1);
    replace_project_chains(&model, &p, &[], &[], &[]);
    assert_eq!(expanded(&model), vec![false, true]);
}

#[test]
fn another_chain_in_the_same_place_starts_collapsed() {
    let model = Rc::new(VecModel::<ProjectChainItem>::default());
    replace_project_chains(&model, &project(vec![chain("A")]), &[], &[], &[]);
    expand_row(&model, 0);
    replace_project_chains(&model, &project(vec![chain("C")]), &[], &[], &[]);
    assert_eq!(expanded(&model), vec![false]);
}
