use std::cell::RefCell;
use std::rc::Rc;

use application::live_source::LiveSource;
use domain::ids::{BlockId, ChainId};
use project::block::types::{AudioBlock, AudioBlockKind, InputBlock};
use project::chain::Chain;
use project::project::Project;
use slint::VecModel;

use super::GuiLiveSource;
use crate::ProjectChainItem;

fn one_chain_project() -> Project {
    Project {
        name: Some("Live".to_string()),
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId("guitar".to_string()),
            description: None,
            instrument: "guitar".to_string(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![AudioBlock {
                id: BlockId("b".to_string()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "default".to_string(),
                    io: String::new(),
                    endpoint: String::new(),
                }),
            }],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    }
}

/// The chain rows the GUI meters write into — one row per project chain,
/// index-aligned, carrying the dBFS the IN/OUT bars drew this tick.
fn rows_with(meters: &[(f32, f32)]) -> Rc<VecModel<ProjectChainItem>> {
    Rc::new(VecModel::from(
        meters
            .iter()
            .map(|(in_dbfs, out_dbfs)| ProjectChainItem {
                meter_in_dbfs: *in_dbfs,
                meter_out_dbfs: *out_dbfs,
                ..Default::default()
            })
            .collect::<Vec<_>>(),
    ))
}

#[test]
fn gui_meters_report_the_values_the_rows_are_showing() {
    // The screen and the transport must not be able to disagree: the
    // reading comes from the row the IN/OUT bars drew this tick, never
    // from a second poll of the audio taps.
    let project = one_chain_project();
    let rows = rows_with(&[(-12.0, -6.0)]);
    let tuner = Rc::new(RefCell::new(None));
    let spectrum = Rc::new(RefCell::new(None));
    let runtime = Rc::new(RefCell::new(None));
    let live = GuiLiveSource {
        project: &project,
        chain_rows: &rows,
        tuner: &tuner,
        spectrum: &spectrum,
        runtime: &runtime,
    };

    let meters = live.chain_meters().expect("the GUI hosts meters");

    assert_eq!(meters.len(), 1);
    assert_eq!(meters[0].chain, ChainId("guitar".to_string()));
    assert_eq!(meters[0].in_dbfs, -12.0);
    assert_eq!(meters[0].out_dbfs, -6.0);
}

#[test]
fn gui_without_a_tuner_session_reports_none_not_a_fabricated_row() {
    // No session ⇒ not hosted. The resolver turns that into the documented
    // `"running":false` payload; the frontend never invents a row.
    let project = one_chain_project();
    let rows = rows_with(&[(-12.0, -6.0)]);
    let tuner = Rc::new(RefCell::new(None));
    let spectrum = Rc::new(RefCell::new(None));
    let runtime = Rc::new(RefCell::new(None));
    let live = GuiLiveSource {
        project: &project,
        chain_rows: &rows,
        tuner: &tuner,
        spectrum: &spectrum,
        runtime: &runtime,
    };

    assert!(live.tuner().is_none());
    assert!(live.spectrum().is_none());
    // No runtime ⇒ no DI playback state and no looper transport state
    // either; both fall back to the resolver's empty shape.
    assert!(live.di_loop().is_none());
    assert!(live.chain_loopers(&ChainId("guitar".to_string())).is_none());
}
