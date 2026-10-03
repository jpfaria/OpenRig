//! Responsibility: opens an independent pipeline's own JACK client.
//!
//! One output port per target channel, each connected to the matching
//! `system:playback_N`, so JACK sums the pipeline with the chains at the
//! playback port. The render fills an interleaved buffer pre-allocated for
//! the largest period JACK can switch to, then the callback spreads it over
//! the ports.

#![cfg(all(target_os = "linux", feature = "jack"))]

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::{anyhow, Result};

use crate::aux_output::{AuxOutputLayout, AuxRender};
use crate::jack_client_open::open_jack_client;
use crate::jack_handlers::JackShutdownHandler;
use crate::jack_server_resolve::resolve_jack_server;
use crate::resolved::MAX_JACK_FRAMES;
use crate::usb_proc::detect_all_usb_audio_cards;

/// An open auxiliary JACK client. Dropping it deactivates and closes it.
pub(crate) struct AuxOutputHandle {
    device_id: String,
    targets: Vec<usize>,
    sample_rate: u32,
    _client: jack::AsyncClient<JackShutdownHandler, AuxJackHandler>,
}

impl AuxOutputHandle {
    /// Whether this client already plays to `device_id` on `targets`.
    pub(crate) fn serves(&self, device_id: &str, targets: &[usize]) -> bool {
        self.device_id == device_id && self.targets == targets
    }

    pub(crate) fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

pub(crate) struct AuxJackHandler {
    ports: Vec<jack::Port<jack::AudioOut>>,
    interleaved: Vec<f32>,
    render: AuxRender,
}

impl jack::ProcessHandler for AuxJackHandler {
    fn process(&mut self, _client: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        let channels = self.ports.len();
        let frames = (ps.n_frames() as usize).min(MAX_JACK_FRAMES);
        let buffer = &mut self.interleaved[..frames * channels];
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (self.render)(buffer))).is_err()
        {
            buffer.fill(0.0);
        }
        for (channel, port) in self.ports.iter_mut().enumerate() {
            let data = port.as_mut_slice(ps);
            for (frame, sample) in data.iter_mut().enumerate().take(frames) {
                *sample = buffer[frame * channels + channel];
            }
        }
        jack::Control::Continue
    }
}

/// Opens and activates a JACK client for one pipeline, with one port per
/// target channel (the first two playback ports when `targets` is empty).
pub(crate) fn open_aux_output(
    device_id: &str,
    targets: &[usize],
    label: &str,
    make_render: impl FnOnce(&AuxOutputLayout) -> AuxRender,
) -> Result<AuxOutputHandle> {
    let server_name = resolve_jack_server(&detect_all_usb_audio_cards(), [device_id]);
    let client_name = format!("openrig_aux_{label}");
    let client = open_jack_client(&server_name, &client_name)?;
    let playback: Vec<usize> = if targets.is_empty() {
        vec![0, 1]
    } else {
        targets.to_vec()
    };
    let mut ports = Vec::with_capacity(playback.len());
    for index in 0..playback.len() {
        let port = client
            .register_port(&format!("out_{}", index + 1), jack::AudioOut::default())
            .map_err(|e| anyhow!("failed to register JACK {label} port {index}: {e:?}"))?;
        ports.push(port);
    }
    let sample_rate = client.sample_rate() as u32;
    let layout = AuxOutputLayout {
        sample_rate,
        channels: playback.len(),
        targets: (0..playback.len()).collect(),
        max_frames: MAX_JACK_FRAMES,
    };
    let handler = AuxJackHandler {
        interleaved: vec![0.0; MAX_JACK_FRAMES * playback.len()],
        ports,
        render: crate::output_fader::faded_render(
            device_id,
            &playback,
            &layout,
            make_render(&layout),
        ),
    };
    let shutdown = JackShutdownHandler {
        shutdown_flag: Arc::new(AtomicBool::new(false)),
    };
    let active = client
        .activate_async(shutdown, handler)
        .map_err(|e| anyhow!("failed to activate JACK {label} client: {e:?}"))?;
    for (index, channel) in playback.iter().enumerate() {
        let src = format!("{client_name}:out_{}", index + 1);
        let dst = format!("system:playback_{}", channel + 1);
        if let Err(e) = active.as_client().connect_ports_by_name(&src, &dst) {
            log::warn!("JACK: failed to connect {src} → {dst}: {e:?}");
        }
    }
    Ok(AuxOutputHandle {
        device_id: device_id.to_string(),
        targets: targets.to_vec(),
        sample_rate,
        _client: active,
    })
}
