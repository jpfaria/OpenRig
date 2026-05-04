use domain::ids::ChainId;
use serde::{Deserialize, Serialize};

use crate::block::{AudioBlock, AudioBlockKind, InputBlock, InputEntry, InsertBlock, OutputBlock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChainOutputMixdown {
    Sum,
    #[default]
    Average,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChainInputMode {
    /// Single-channel input; upmixed to stereo for stereo outputs.
    #[default]
    #[serde(alias = "auto")]
    Mono,
    /// Two-channel input treated as a true stereo L/R pair.
    Stereo,
    /// Two independent mono pipelines (e.g. two guitars on separate inputs).
    DualMono,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChainOutputMode {
    Mono,
    #[default]
    Stereo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingLayout {
    Mono,
    Stereo,
    DualMono,
}

/// Determines the audio processing layout from the combination
/// of input channels, output channels, and input mode.
pub fn processing_layout(
    input_channels: &[usize],
    output_channels: &[usize],
    input_mode: ChainInputMode,
) -> ProcessingLayout {
    let in_count = input_channels.len();
    let out_count = output_channels.len();

    // Dual mono: 2 independent mono pipelines
    if in_count >= 2 && matches!(input_mode, ChainInputMode::DualMono) {
        return ProcessingLayout::DualMono;
    }

    // Stereo input: always process as stereo
    if matches!(input_mode, ChainInputMode::Stereo) {
        return ProcessingLayout::Stereo;
    }

    // Mono input: output channel count determines final layout (upmix if needed)
    match out_count {
        0 | 1 => ProcessingLayout::Mono,
        _ => ProcessingLayout::Stereo,
    }
}

/// Determines the processing layout from an InputEntry.
pub fn processing_layout_for_input_entry(entry: &InputEntry) -> ProcessingLayout {
    let ch_count = entry.channels.len();
    match entry.mode {
        ChainInputMode::DualMono if ch_count >= 2 => ProcessingLayout::DualMono,
        ChainInputMode::Stereo if ch_count >= 2 => ProcessingLayout::Stereo,
        _ => ProcessingLayout::Mono,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Chain {
    #[serde(skip)]
    pub id: ChainId,
    #[serde(default)]
    pub description: Option<String>,
    pub instrument: String,
    pub enabled: bool,
    #[serde(default)]
    pub blocks: Vec<AudioBlock>,
}

impl Chain {
    /// Returns all Input blocks with their indices in the blocks vec.
    pub fn input_blocks(&self) -> Vec<(usize, &InputBlock)> {
        self.blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| match &b.kind {
                AudioBlockKind::Input(input) => Some((i, input)),
                _ => None,
            })
            .collect()
    }

    /// Returns all Insert blocks with their indices in the blocks vec.
    pub fn insert_blocks(&self) -> Vec<(usize, &InsertBlock)> {
        self.blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| match &b.kind {
                AudioBlockKind::Insert(insert) => Some((i, insert)),
                _ => None,
            })
            .collect()
    }

    /// Returns all Output blocks with their indices in the blocks vec.
    pub fn output_blocks(&self) -> Vec<(usize, &OutputBlock)> {
        self.blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| match &b.kind {
                AudioBlockKind::Output(output) => Some((i, output)),
                _ => None,
            })
            .collect()
    }

    /// Returns the first Input block, if any.
    pub fn first_input(&self) -> Option<&InputBlock> {
        self.blocks.iter().find_map(|b| match &b.kind {
            AudioBlockKind::Input(input) => Some(input),
            _ => None,
        })
    }

    /// Returns the last Output block, if any.
    pub fn last_output(&self) -> Option<&OutputBlock> {
        self.blocks.iter().rev().find_map(|b| match &b.kind {
            AudioBlockKind::Output(output) => Some(output),
            _ => None,
        })
    }

    /// Validate that no two input entries share the same device+channel,
    /// and no two output entries share the same device+channel.
    pub fn validate_channel_conflicts(&self) -> Result<(), String> {
        let mut used: Vec<(String, usize)> = Vec::new();
        for (_, input) in self.input_blocks() {
            for entry in &input.entries {
                for &ch in &entry.channels {
                    let key = (entry.device_id.0.clone(), ch);
                    if used.contains(&key) {
                        return Err(format!(
                            "Channel {} on device '{}' is used by multiple inputs",
                            ch, entry.device_id.0
                        ));
                    }
                    used.push(key);
                }
            }
        }
        let mut used: Vec<(String, usize)> = Vec::new();
        for (_, output) in self.output_blocks() {
            for entry in &output.entries {
                for &ch in &entry.channels {
                    let key = (entry.device_id.0.clone(), ch);
                    if used.contains(&key) {
                        return Err(format!(
                            "Channel {} on device '{}' is used by multiple outputs",
                            ch, entry.device_id.0
                        ));
                    }
                    used.push(key);
                }
            }
        }
        Ok(())
    }

    /// Migration for projects saved while issue #377 was open: collapse a run of
    /// consecutive `InputBlock`s at the chain head into a single block, and a
    /// run of consecutive `OutputBlock`s at the chain tail into a single block.
    /// All entries are preserved in source order, so the runtime keeps spawning
    /// the same number of streams. Non-consecutive I/O blocks (e.g. an Input
    /// flanked by effects) are intentionally left alone — only the boundary
    /// runs that the bug produced are merged.
    pub fn coalesce_endpoint_blocks(&mut self) {
        let head_run = self
            .blocks
            .iter()
            .take_while(|b| matches!(&b.kind, AudioBlockKind::Input(_)))
            .count();
        if head_run > 1 {
            let mut merged_entries: Vec<InputEntry> = Vec::new();
            for block in self.blocks.iter().take(head_run) {
                if let AudioBlockKind::Input(ib) = &block.kind {
                    merged_entries.extend(ib.entries.iter().cloned());
                }
            }
            // Keep the first block's id and model so any externally-held reference
            // (e.g. selection state) keeps working.
            if let AudioBlockKind::Input(ib) = &mut self.blocks[0].kind {
                ib.entries = merged_entries;
            }
            self.blocks.drain(1..head_run);
        }

        let tail_run = self
            .blocks
            .iter()
            .rev()
            .take_while(|b| matches!(&b.kind, AudioBlockKind::Output(_)))
            .count();
        if tail_run > 1 {
            let len = self.blocks.len();
            let tail_start = len - tail_run;
            let mut merged_entries: Vec<crate::block::OutputEntry> = Vec::new();
            for block in self.blocks.iter().skip(tail_start) {
                if let AudioBlockKind::Output(ob) = &block.kind {
                    merged_entries.extend(ob.entries.iter().cloned());
                }
            }
            // Keep the last block as the survivor (preserves rposition-based id).
            let last_idx = len - 1;
            if let AudioBlockKind::Output(ob) = &mut self.blocks[last_idx].kind {
                ob.entries = merged_entries;
            }
            self.blocks.drain(tail_start..last_idx);
        }
    }
}

#[cfg(test)]
#[path = "chain_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "chain_coalesce_tests.rs"]
mod coalesce_tests;
