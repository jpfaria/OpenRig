//! Responsibility: identifies one global-mixer strip by the physical endpoint it controls.
//!
//! A strip is keyed by (direction, device, channels) — never by binding —
//! so a physical output declared in several I/O bindings is ONE fader
//! (issue #1007). The id travels as a string over the command bus, MCP and
//! MIDI bindings: `in:<ch,ch>@<device_id>` / `out:<ch,ch>@<device_id>`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::io_binding::ChannelMode;

/// Which side of the rig a strip controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MixerDirection {
    Input,
    Output,
}

/// The physical endpoint one strip controls.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MixerStripId {
    pub direction: MixerDirection,
    pub device_id: String,
    pub channels: Vec<usize>,
}

impl MixerStripId {
    /// Wire form: `in:0,1@<device>` / `out:0,1@<device>`.
    pub fn to_wire(&self) -> String {
        let prefix = match self.direction {
            MixerDirection::Input => "in",
            MixerDirection::Output => "out",
        };
        let channels: Vec<String> = self.channels.iter().map(usize::to_string).collect();
        format!("{prefix}:{}@{}", channels.join(","), self.device_id)
    }

    /// Parse the wire form; `None` when it is not a strip id.
    pub fn parse(wire: &str) -> Option<Self> {
        let (head, device_id) = wire.split_once('@')?;
        let (prefix, channels) = head.split_once(':')?;
        let direction = match prefix {
            "in" => MixerDirection::Input,
            "out" => MixerDirection::Output,
            _ => return None,
        };
        let channels = channels
            .split(',')
            .map(|c| c.parse::<usize>().ok())
            .collect::<Option<Vec<usize>>>()?;
        if device_id.is_empty() {
            return None;
        }
        Some(Self {
            direction,
            device_id: device_id.to_string(),
            channels,
        })
    }

    /// The channel groups the engine sees for this endpoint. A mono input
    /// with N > 1 channels runs as N single-channel pipelines, so the fader
    /// value has to reach each of them as well as the whole group.
    pub fn runtime_channel_groups(&self, mode: ChannelMode) -> Vec<Vec<usize>> {
        let mut groups = vec![self.channels.clone()];
        if self.direction == MixerDirection::Input
            && mode == ChannelMode::Mono
            && self.channels.len() > 1
        {
            groups.extend(self.channels.iter().map(|&c| vec![c]));
        }
        groups
    }
}
