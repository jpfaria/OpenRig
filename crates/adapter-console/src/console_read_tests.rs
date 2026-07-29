use std::cell::RefCell;
use std::rc::Rc;

use application::bridge::QueryKind;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::{BlockId, ChainId};
use project::block::types::{AudioBlock, AudioBlockKind, InputBlock};
use project::chain::Chain;
use project::project::Project;

use super::console_resolve;

fn one_chain_project() -> Project {
    Project {
        name: Some("Console".to_string()),
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

fn rc(project: &Project) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(project.clone()))
}

#[test]
fn console_serves_meters_with_the_engine_silent_constant() {
    let project = one_chain_project();
    let dispatcher = LocalDispatcher::new(rc(&project));
    let out = console_resolve(&QueryKind::ChainMeters, &project, &[], &dispatcher)
        .expect("chain meters always answer");
    assert!(
        !out.contains("-120.0") || engine::output_meter::SILENT_DBFS == -120.0,
        "console must not hardcode the silent value"
    );
    assert_eq!(
        out,
        format!(
            "guitar\t{:.1}\t{:.1}\n",
            engine::output_meter::SILENT_DBFS,
            engine::output_meter::SILENT_DBFS
        )
    );
}

/// #829/#831: a read the console hosts no live source for must still answer
/// the documented empty shape, not refuse with a console-specific string.
#[test]
fn console_answers_reads_it_does_not_host_instead_of_refusing() {
    let project = one_chain_project();
    let dispatcher = LocalDispatcher::new(rc(&project));
    let out = console_resolve(&QueryKind::TunerReadings, &project, &[], &dispatcher)
        .expect("an unhosted read must still answer with the empty shape");
    assert!(out.contains("\"running\":false"), "got: {out}");
}
