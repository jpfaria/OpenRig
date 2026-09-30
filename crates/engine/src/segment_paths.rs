//! Responsibility: maps each output route of a chain to the split paths it runs.

use domain::io_binding::IoBinding;
use project::binding_discovery::{resolve_chain_ports, PortDirection};
use project::chain::Chain;
use project::endpoint_feeds::{tail_feed, TailFeed};

pub use crate::segment_types::SegmentPaths;

/// The split paths each resolved output route runs, in route order — the
/// order `resolve_chain_ports` numbers outputs with, which is the runtime's
/// route order (`classify_output_routes` walks the same list). A tail route
/// of a Y → A/B chain runs the paths whose output node has that endpoint
/// checked. A mid `Output` has no path node, so its route is `None`. Every
/// route of a chain without a Y split is `None`.
pub fn route_paths(chain: &Chain, registry: &[IoBinding]) -> Vec<SegmentPaths> {
    let tail = chain.blocks.len();
    resolve_chain_ports(chain, registry)
        .into_iter()
        .filter(|port| port.direction == PortDirection::Output)
        .map(|port| {
            if port.offset < tail {
                return SegmentPaths::None;
            }
            match tail_feed(chain, &port.binding_id, &port.endpoint.name) {
                TailFeed::Paths { a: true, b: true } => SegmentPaths::AB,
                TailFeed::Paths { a: true, b: false } => SegmentPaths::A,
                TailFeed::Paths { a: false, b: true } => SegmentPaths::B,
                TailFeed::Paths { a: false, b: false } | TailFeed::Chain | TailFeed::Off => {
                    SegmentPaths::None
                }
            }
        })
        .collect()
}
