//! #716 RED — a chain bound via the system E/S binding (io_binding_ids) must
//! count as having bound ports.
//!
//! `sync_project` skips any chain where `chain_has_bound_ports(chain)` is false
//! ("an unbound chain opens nothing"). The function only inspects Input/Output
//! BLOCKS with `io` set — but a checklist-bound chain carries no I/O blocks
//! (its I/O is the binding reference). So it is judged unbound → skipped → no
//! runtime → no sound on activate.

use domain::ids::ChainId;
use engine::io_routing::chain_has_bound_ports;
use project::chain::Chain;

#[test]
fn chain_with_io_binding_ids_has_bound_ports() {
    let chain = Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![],
    };

    assert!(
        chain_has_bound_ports(&chain),
        "a chain referencing an E/S binding (io_binding_ids) has bound I/O and must be \
         recognized as having bound ports; otherwise sync_project skips it and the chain \
         never gets a runtime (no sound on activate)"
    );
}
