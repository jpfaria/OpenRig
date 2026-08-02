//! #85 — the live rebuild must know EVERY device's rate, outputs included.
//!
//! "Quando inicia a chain pela primeira vez o som fica bom; se eu alterar a
//! ordem dos blocos começa a ficar ruim na TEYUN." Only a live edit goes
//! through the off-thread rebuild, and it derives the per-device rates from the
//! stream signature. Taking them from the INPUTS alone leaves a mid `Output` on
//! another interface without its rate: the route falls back to the chain's, the
//! conversion stops, and the tap is fed at the wrong pace.

use domain::ids::DeviceId;

use super::controller_offthread_live_rebuild::device_rates_from_signature;
use super::resolved::{ChainStreamSignature, InputStreamSignature, OutputStreamSignature};

const SCARLETT: &str = "coreaudio:scarlett";
const TEYUN: &str = "coreaudio:teyun";

/// His rig: the chain clocked by the Scarlett at 44.1 kHz, the mid `Output`
/// writing to the TEYUN at 48 kHz.
fn signature() -> ChainStreamSignature {
    ChainStreamSignature {
        inputs: vec![InputStreamSignature {
            device_id: SCARLETT.into(),
            channels: vec![0],
            stream_channels: 2,
            sample_rate: 44_100,
            buffer_size_frames: 64,
        }],
        outputs: vec![
            OutputStreamSignature {
                device_id: SCARLETT.into(),
                channels: vec![0, 1],
                stream_channels: 2,
                sample_rate: 44_100,
                buffer_size_frames: 64,
            },
            OutputStreamSignature {
                device_id: TEYUN.into(),
                channels: vec![0, 1],
                stream_channels: 2,
                sample_rate: 48_000,
                buffer_size_frames: 64,
            },
        ],
    }
}

#[test]
fn the_rebuild_knows_the_tap_devices_rate() {
    let rates = device_rates_from_signature(&signature());
    assert_eq!(
        rates.get(&DeviceId(TEYUN.into())).copied(),
        Some(48_000.0),
        "#85: the rebuild must carry the OUTPUT devices' rates — without the \
         TEYUN's, the tap's route falls back to the chain's rate and stops \
         converting after the first live edit"
    );
}

#[test]
fn it_still_knows_the_chains_own_rate() {
    let rates = device_rates_from_signature(&signature());
    assert_eq!(
        rates.get(&DeviceId(SCARLETT.into())).copied(),
        Some(44_100.0)
    );
}
