//! Responsibility: keeps the historical `runtime_endpoints` path pointing at the four things it held.
//!
//! It was responsible for the resolved endpoint entries, the input-channel
//! conflict rules, expanding those entries into streams, and the insert
//! shims (#873).

pub(crate) use crate::effective_endpoints::{effective_inputs, effective_outputs};
pub use crate::endpoint_entry::{
    resolve_chain_io, resolve_chain_io_by_binding, BindingIo, InputEntry, OutputEntry,
};
pub(crate) use crate::insert_endpoints::insert_is_bound;

// `runtime.rs` and the test module below reach the insert shims through this
// path, where they were defined before the split (#873).
pub use crate::input_conflicts::{
    conflicting_input_channel, disable_conflicting_chains, input_conflicting_chains,
    input_port_conflict, InputChannelConflict,
};
#[cfg(test)]
pub(crate) use crate::insert_endpoints::{
    insert_return_as_input_entry, insert_send_as_output_entry,
};

#[cfg(test)]
mod insert_binding_tests {
    use super::*;
    use domain::ids::DeviceId;
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    use project::block::InsertBlock;

    fn fx_binding() -> Vec<IoBinding> {
        vec![IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![IoEndpoint {
                name: "ret".into(),
                device_id: DeviceId("dev".into()),
                mode: ChannelMode::Mono,
                channels: vec![2],
            }],
            outputs: vec![IoEndpoint {
                name: "snd".into(),
                device_id: DeviceId("dev".into()),
                mode: ChannelMode::Stereo,
                channels: vec![0, 1],
            }],
        }]
    }

    #[test]
    fn insert_send_and_return_resolve_from_its_binding() {
        let insert = InsertBlock {
            model: "external_loop".into(),
            io: "fx".into(),
        };
        let reg = fx_binding();
        let snd = insert_send_as_output_entry(&insert, &reg).expect("send resolves");
        assert_eq!(snd.device_id.0, "dev");
        assert_eq!(snd.channels, vec![0, 1]);
        let ret = insert_return_as_input_entry(&insert, &reg).expect("return resolves");
        assert_eq!(ret.device_id.0, "dev");
        assert_eq!(ret.channels, vec![2]);
    }

    #[test]
    fn insert_with_unknown_binding_resolves_to_nothing() {
        let insert = InsertBlock {
            model: "external_loop".into(),
            io: "ghost".into(),
        };
        assert!(insert_send_as_output_entry(&insert, &fx_binding()).is_none());
        assert!(insert_return_as_input_entry(&insert, &fx_binding()).is_none());
    }
}
