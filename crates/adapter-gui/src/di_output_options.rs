//! #771: pure option list for the DI panel's OUTPUT select — the chain's
//! already-bound output endpoints, in the SAME flat order
//! `engine::di_output_resolve::resolve_di_output_index` numbers them with
//! (both walk `resolve_chain_ports`), so a picked index maps 1:1 to the
//! output the playback parks on.

use domain::io_binding::IoBinding;
use project::binding_discovery::{resolve_chain_ports, PortDirection};
use project::chain::{Chain, DiOutputRef};

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

#[cfg(test)]
#[path = "di_output_options_tests.rs"]
mod tests;

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
