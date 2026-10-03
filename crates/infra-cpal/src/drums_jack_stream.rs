//! Responsibility: plays the drum machine through its own JACK client.
//!
//! The drums get a client of their own (`openrig_drums`) with one port per
//! channel of the chosen endpoint, connected to that channel's playback
//! port. JACK sums them with whatever else plays there; no chain's client
//! or buffers ever see the groove.

#![cfg(all(target_os = "linux", feature = "jack"))]

use std::sync::Arc;

use anyhow::{anyhow, Result};

use engine::drum_state::DrumsShared;

use crate::drums_callback::DrumsCallback;
use crate::drums_jack_ports::{drums_jack_ports, jack_server_for_device};
use crate::jack_client_open::open_jack_client;
use crate::resolved::MAX_JACK_FRAMES;
use crate::usb_proc::detect_all_usb_audio_cards;

const CLIENT_NAME: &str = "openrig_drums";
/// The callback writes its pair to these channels of the port scratch.
const PORT_CHANNELS: [usize; 2] = [0, 1];

pub(crate) type DrumsJackClient = jack::AsyncClient<(), DrumsJackProcess>;

pub(crate) struct DrumsJackProcess {
    callback: DrumsCallback,
    shared: Arc<DrumsShared>,
    ports: Vec<jack::Port<jack::AudioOut>>,
    /// Interleaved across `ports`, sized for MAX_JACK_FRAMES so a larger
    /// JACK buffer never reallocates on the audio thread.
    scratch: Vec<f32>,
}

impl jack::ProcessHandler for DrumsJackProcess {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        let channels = self.ports.len();
        let frames = (ps.n_frames() as usize).min(MAX_JACK_FRAMES);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let scratch = &mut self.scratch[..frames * channels];
            self.callback
                .fill(&self.shared, scratch, channels, &PORT_CHANNELS[..channels]);
            for (ch, port) in self.ports.iter_mut().enumerate() {
                let out = port.as_mut_slice(ps);
                for (sample, frame) in out.iter_mut().zip(scratch.chunks(channels)) {
                    *sample = frame[ch];
                }
                // Past MAX_JACK_FRAMES the port would replay stale samples.
                out.iter_mut().skip(frames).for_each(|s| *s = 0.0);
            }
        }));
        jack::Control::Continue
    }
}

/// Open the drums' client on the server of `device_id`, routed to
/// `target_channels`, and return it with the server's sample rate.
pub(crate) fn open_drums_jack(
    device_id: &str,
    target_channels: &[usize],
    shared: &Arc<DrumsShared>,
) -> Result<(DrumsJackClient, u32)> {
    let cards = detect_all_usb_audio_cards();
    let server_name = jack_server_for_device(device_id, |hw_num| {
        cards
            .iter()
            .find(|c| c.card_num == hw_num)
            .map(|c| c.server_name.clone())
    })
    .ok_or_else(|| anyhow!("drums output '{device_id}' is not a JACK device"))?;
    let client = open_jack_client(&server_name, CLIENT_NAME)?;
    let sample_rate = client.sample_rate() as u32;

    let layout = drums_jack_ports(target_channels);
    let mut ports = Vec::with_capacity(layout.len());
    for port in &layout {
        ports.push(
            client
                .register_port(&port.name, jack::AudioOut::default())
                .map_err(|e| anyhow!("failed to register drums port {}: {e:?}", port.name))?,
        );
    }
    let process = DrumsJackProcess {
        callback: DrumsCallback::new(shared, sample_rate, MAX_JACK_FRAMES),
        shared: Arc::clone(shared),
        scratch: vec![0.0; MAX_JACK_FRAMES * ports.len()],
        ports,
    };
    let active = client
        .activate_async((), process)
        .map_err(|e| anyhow!("failed to activate the drums JACK client: {e:?}"))?;
    // While an output change opens this client the old one still plays, so
    // JACK may have suffixed the name.
    let name = active.as_client().name().to_string();
    for port in &layout {
        let src = format!("{name}:{}", port.name);
        if let Err(e) = active
            .as_client()
            .connect_ports_by_name(&src, &port.playback)
        {
            log::warn!("JACK: failed to connect {src} → {}: {e:?}", port.playback);
        }
    }
    log::info!("drums: JACK client on server '{server_name}' at {sample_rate} Hz");
    Ok((active, sample_rate))
}
