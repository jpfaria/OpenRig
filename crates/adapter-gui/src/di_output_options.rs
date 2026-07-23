//! #771: pure option list for the DI panel's OUTPUT select — the chain's
//! already-bound output endpoints, in the SAME flat order
//! `engine::di_output_resolve::resolve_di_output_index` numbers them with
//! (both walk `resolve_chain_ports`), so a picked index maps 1:1 to the
//! output the playback parks on.

use std::rc::Rc;

use domain::io_binding::IoBinding;
use project::binding_discovery::{resolve_chain_ports, PortDirection};
use project::chain::{Chain, DiOutputRef};
use project::project::Project;
use slint::{Model, ModelRc, SharedString, VecModel};

use crate::ProjectChainItem;

/// One pickable output endpoint: the persisted reference + its display label.
#[derive(Debug, Clone, PartialEq)]
pub struct DiOutputOption {
    pub di_ref: DiOutputRef,
    pub label: String,
}

/// The chain's bound output endpoints in flat resolve order. An endpoint
/// name that repeats across the options (two interfaces both exposing an
/// "Out 1") gets its binding's name prefixed, so the select never shows two
/// identical rows; the persisted ref keeps the raw endpoint name.
pub fn build_di_output_options(chain: &Chain, registry: &[IoBinding]) -> Vec<DiOutputOption> {
    let ports: Vec<_> = resolve_chain_ports(chain, registry)
        .into_iter()
        .filter(|p| p.direction == PortDirection::Output)
        .collect();
    ports
        .iter()
        .map(|p| {
            let duplicated = ports
                .iter()
                .filter(|q| q.endpoint.name == p.endpoint.name)
                .count()
                > 1;
            let label = if duplicated {
                let binding_name = registry
                    .iter()
                    .find(|b| b.id == p.binding_id)
                    .map(|b| b.name.trim())
                    .unwrap_or(p.binding_id.as_str());
                format!("{} · {}", binding_name, p.endpoint.name)
            } else {
                p.endpoint.name.clone()
            };
            DiOutputOption {
                di_ref: DiOutputRef {
                    binding_id: p.binding_id.clone(),
                    endpoint: p.endpoint.name.clone(),
                },
                label,
            }
        })
        .collect()
}

/// Row-model convenience: the labels plus the selected index in one call
/// (what `ProjectChainItem.di_loop_outputs` / `.di_output_selected_index`
/// carry).
pub fn output_labels_and_index(chain: &Chain, registry: &[IoBinding]) -> (Vec<String>, i32) {
    let options = build_di_output_options(chain, registry);
    let index = di_output_selected_index(chain, &options);
    (options.into_iter().map(|o| o.label).collect(), index)
}

/// Index of the chain's persisted `di_output` within `options`; `None` or a
/// stale reference select the main output (index 0, today's default).
pub fn di_output_selected_index(chain: &Chain, options: &[DiOutputOption]) -> i32 {
    if options.is_empty() {
        return -1;
    }
    chain
        .di_output
        .as_ref()
        .and_then(|r| options.iter().position(|o| o.di_ref == *r))
        .unwrap_or(0) as i32
}

/// #808: refresh the DI output select for EVERY chain row from the (real) I/O
/// bindings — regardless of whether the chain is active/enabled. The DI panel's
/// options are built inside `replace_project_chains`, but every caller passes an
/// empty binding registry, so a freshly opened project shows an EMPTY select;
/// the meter timer refreshes it with the real bindings but ONLY for chains that
/// produce a live audio reading, so a chain that was never enabled stayed empty
/// until the first enable. The options come from the chain's bindings (offline,
/// no device or active stream needed — invariant #4), so populate them for all
/// chains here. Only the touched fields are written, and only when they change.
pub(crate) fn apply_di_outputs_to_rows(
    model: &VecModel<ProjectChainItem>,
    project: &Project,
    io_bindings: &[IoBinding],
) {
    for (idx, chain) in project.chains.iter().enumerate() {
        let Some(mut row) = model.row_data(idx) else {
            continue;
        };
        let (labels, selected) = output_labels_and_index(chain, io_bindings);
        let current: Vec<String> = row.di_loop_outputs.iter().map(|s| s.to_string()).collect();
        let labels_changed = current != labels;
        let index_changed = row.di_output_selected_index != selected;
        if !labels_changed && !index_changed {
            continue;
        }
        if labels_changed {
            row.di_loop_outputs = ModelRc::from(Rc::new(VecModel::from(
                labels
                    .into_iter()
                    .map(SharedString::from)
                    .collect::<Vec<_>>(),
            )));
        }
        if index_changed {
            row.di_output_selected_index = selected;
        }
        model.set_row_data(idx, row);
    }
}

#[cfg(test)]
#[path = "di_output_options_tests.rs"]
mod tests;

#[cfg(test)]
mod di_output_select_before_enable_808_tests {
    use super::*;
    use domain::ids::{ChainId, DeviceId};
    use domain::io_binding::{ChannelMode, IoEndpoint};

    fn registry() -> Vec<IoBinding> {
        vec![IoBinding {
            id: "io".into(),
            name: "IO".into(),
            inputs: vec![],
            outputs: vec![
                IoEndpoint {
                    name: "Main Out".into(),
                    device_id: DeviceId("dev".into()),
                    mode: ChannelMode::Stereo,
                    channels: vec![0, 1],
                },
                IoEndpoint {
                    name: "FX Out".into(),
                    device_id: DeviceId("dev".into()),
                    mode: ChannelMode::Stereo,
                    channels: vec![2, 3],
                },
            ],
        }]
    }

    fn disabled_chain() -> Chain {
        Chain {
            id: ChainId("c808".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: false, // opened the project, never enabled this chain
            volume: 100.0,
            io_binding_ids: vec!["io".into()],
            blocks: vec![],
            di_output: None,
            loopers: vec![],
        }
    }

    /// #808 (owner): "open the project without ever enabling the chain, open the
    /// DI — the output select does not appear; only after I enable the chain the
    /// first time." The select's options are built inside `replace_project_chains`
    /// from its `io_bindings` arg, which every caller passes EMPTY, so the row
    /// opens with no outputs. The refresh that keeps it fresh must populate it
    /// from the real bindings even while the chain is disabled (the options are
    /// offline — invariant #4).
    #[test]
    fn di_output_select_populates_before_the_chain_is_ever_enabled() {
        let project = Project {
            name: None,
            device_settings: vec![],
            chains: vec![disabled_chain()],
            midi: None,
        };
        let model: Rc<VecModel<ProjectChainItem>> = Rc::new(VecModel::default());
        // Reproduce the open flow: rows built with an EMPTY binding registry.
        crate::project_view::replace_project_chains(&model, &project, &[], &[], &[]);
        let before = model.row_data(0).unwrap().di_loop_outputs.iter().count();
        assert_eq!(before, 0, "precondition: the open flow leaves the DI select empty");

        // The refresh must fill the select from the real bindings — disabled or not.
        apply_di_outputs_to_rows(&model, &project, &registry());

        let after = model.row_data(0).unwrap().di_loop_outputs.iter().count();
        assert_eq!(
            after, 2,
            "#808: the DI output select must list the chain's two bound outputs \
             even though the chain was never enabled — it stayed empty until the \
             first enable."
        );
    }
}

#[cfg(test)]
mod duplicate_label_tests {
    use super::*;
    use domain::ids::{ChainId, DeviceId};
    use domain::io_binding::{ChannelMode, IoEndpoint};

    fn out(name: &str) -> IoEndpoint {
        IoEndpoint {
            name: name.into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }
    }

    /// Owner report (#771): two interfaces each expose an endpoint named
    /// "Out 1" — the select showed "OUT 1 / OUT 1", indistinguishable. When
    /// an endpoint name repeats across the options, the label must carry the
    /// binding's name so the user can tell the outputs apart.
    #[test]
    fn duplicate_endpoint_names_get_binding_qualified_labels() {
        let chain = Chain {
            id: ChainId("di771dup".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["scarlett".into(), "teyun".into()],
            blocks: vec![],
            di_output: None,
            loopers: vec![],
        };
        let registry = vec![
            IoBinding {
                id: "scarlett".into(),
                name: "SCARLET".into(),
                inputs: vec![],
                outputs: vec![out("Out 1")],
            },
            IoBinding {
                id: "teyun".into(),
                name: "TEYUN".into(),
                inputs: vec![],
                outputs: vec![out("Out 1")],
            },
        ];
        let options = build_di_output_options(&chain, &registry);
        assert_eq!(options.len(), 2);
        assert_ne!(
            options[0].label, options[1].label,
            "duplicate endpoint names must yield distinguishable labels"
        );
        assert_eq!(options[0].label, "SCARLET · Out 1");
        assert_eq!(options[1].label, "TEYUN · Out 1");
        // The persisted refs stay the raw endpoint names (label-only change).
        assert_eq!(options[0].di_ref.endpoint, "Out 1");
        assert_eq!(options[1].di_ref.endpoint, "Out 1");
    }

    /// Unique endpoint names keep their plain label (no binding prefix).
    #[test]
    fn unique_endpoint_names_keep_plain_labels() {
        let chain = Chain {
            id: ChainId("di771uniq".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["io".into()],
            blocks: vec![],
            di_output: None,
            loopers: vec![],
        };
        let registry = vec![IoBinding {
            id: "io".into(),
            name: "IO".into(),
            inputs: vec![],
            outputs: vec![out("Main Out"), out("FX Out")],
        }];
        let options = build_di_output_options(&chain, &registry);
        assert_eq!(options[0].label, "Main Out");
        assert_eq!(options[1].label, "FX Out");
    }
}
