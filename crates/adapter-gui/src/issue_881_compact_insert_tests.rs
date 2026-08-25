//! #881 — the compact view must show the routing blocks the chain carries.
//!
//! `build_compact_blocks` maps each block through `block_editor_data`, which
//! answers `None` for anything without a model — an `Insert` and the mid
//! `Input`/`Output` ports. They were silently dropped, so a chain whose loop
//! sits between two pedals showed only the pedals: the insert cannot be seen,
//! selected, bypassed or removed from the compact view, and the row order no
//! longer matches the signal path the user built.

use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InsertBlock, OutputBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

use crate::compact_block_view::build_compact_blocks;
use slint::Model;

fn drive(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn insert(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "external_loop".into(),
            io: "fx".into(),
        }),
    }
}

fn mid_output(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "Out 1".into(),
        }),
    }
}

fn registry() -> Vec<infra_filesystem::IoBinding> {
    use domain::ids::DeviceId;
    use domain::io_binding::{ChannelMode, IoEndpoint};
    let ep = |name: &str| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Mono,
        channels: vec![0],
    };
    vec![
        infra_filesystem::IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![ep("In 1")],
            outputs: vec![ep("Out 1")],
        },
        infra_filesystem::IoBinding {
            id: "fx".into(),
            name: "SYNERGY".into(),
            inputs: vec![ep("ret")],
            outputs: vec![ep("snd")],
        },
    ]
}

fn project_with(blocks: Vec<AudioBlock>) -> Project {
    Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("rig:input-1".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks,
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    }
}

#[test]
fn the_compact_view_lists_the_insert_between_the_pedals() {
    let project = project_with(vec![drive("a"), insert("loop"), drive("b")]);

    let items = build_compact_blocks(&project, 0, &registry());
    let ids: Vec<String> = items.iter().map(|i| i.block_id.to_string()).collect();

    assert_eq!(
        ids,
        vec!["a".to_string(), "loop".to_string(), "b".to_string()],
        "#881: the insert is a row of the chain like any other — dropping it \
         hides the loop and breaks the order the user sees"
    );
    let loop_row = &items[1];
    assert_eq!(loop_row.effect_type.to_string(), "insert");
    assert_eq!(
        loop_row.icon_kind.to_string(),
        "insert",
        "the row must draw the insert icon, not the generic gear"
    );
    assert_eq!(
        loop_row.display_label.to_string(),
        "INSERT",
        "the label says WHICH port it is"
    );
    // #881: the model slot carries the E/S bindings, so the loop can be
    // re-pointed from the compact view.
    let options: Vec<String> = loop_row
        .models
        .iter()
        .map(|m| m.model_id.to_string())
        .collect();
    assert_eq!(
        options,
        vec!["main".to_string(), "fx".to_string()],
        "the picker must offer the E/S bindings"
    );
    assert_eq!(
        loop_row.model_selected_index, 1,
        "and open on the one the insert points at"
    );
    assert_eq!(
        loop_row.block_index, 1,
        "the row must carry the REAL chain index, or select/reorder/delete hit \
         the wrong block"
    );
}

#[test]
fn the_compact_view_lists_a_mid_port_too() {
    let project = project_with(vec![drive("a"), mid_output("aux"), drive("b")]);

    let items = build_compact_blocks(&project, 0, &[]);
    let ids: Vec<String> = items.iter().map(|i| i.block_id.to_string()).collect();

    assert_eq!(
        ids,
        vec!["a".to_string(), "aux".to_string(), "b".to_string()]
    );
    assert_eq!(items[1].effect_type.to_string(), "output");
}
