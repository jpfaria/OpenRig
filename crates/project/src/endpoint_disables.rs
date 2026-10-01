//! Responsibility: records which endpoints of a chain's own bindings each graph node leaves out.
//!
//! #328 (spec §1.3, §11.3): the input and output nodes of a chain's graph list
//! every endpoint of the chain's E/S bindings, checked by default. Unchecking
//! one leaves it out of THAT node only — it stays listed, and nothing is
//! removed from the E/S. Every Y leaf owns its own output node, addressed by
//! [`PathRef`]. This is chain configuration, not preset data (one preset is
//! reused by several inputs), so it lives on `RigInput` and on the projected
//! `Chain`.

use domain::ids::BlockId;
use serde::{Deserialize, Serialize};

use crate::block::{find_split_with_end, AudioBlock, PathRef, SplitEnd};

/// One endpoint of one of the chain's own E/S bindings, as a checklist row
/// addresses it: the binding id (`io`, the same field name as `InputBlock.io`)
/// plus the endpoint name — a name alone is not unique across bindings.
/// Not the looper's `crate::endpoint_ref::EndpointRef` (#323), which keeps its
/// own `binding_id` field; where both are in scope, spell this one
/// `project::endpoint_disables::EndpointRef`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EndpointRef {
    pub io: String,
    pub endpoint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EndpointNode {
    /// The chain's input node.
    Input,
    /// The chain's output node (chains without a Y split).
    Output,
    /// The output node of one Y leaf.
    PathOutput(PathRef),
}

/// The unchecked endpoints of one Y leaf's output node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PathOutputDisables {
    pub split: BlockId,
    pub path: usize,
    #[serde(default)]
    pub disabled: Vec<EndpointRef>,
}

/// The unchecked endpoints, per node. Empty — the default — keeps every
/// endpoint; empty nodes are not written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EndpointDisables {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<EndpointRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<EndpointRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_outputs: Vec<PathOutputDisables>,
    /// Files saved before §11 named the two paths of the chain's single Y;
    /// [`EndpointDisables::adopt_legacy_paths`] moves them onto that Y.
    #[serde(default, skip_serializing, rename = "path_a_outputs")]
    legacy_path_a_outputs: Vec<EndpointRef>,
    #[serde(default, skip_serializing, rename = "path_b_outputs")]
    legacy_path_b_outputs: Vec<EndpointRef>,
}

impl EndpointDisables {
    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
            && self.outputs.is_empty()
            && self
                .path_outputs
                .iter()
                .all(|leaf| leaf.disabled.is_empty())
            && self.legacy_path_a_outputs.is_empty()
            && self.legacy_path_b_outputs.is_empty()
    }

    /// Whether `node` keeps the endpoint `r` (checked).
    pub fn is_enabled(&self, node: &EndpointNode, r: &EndpointRef) -> bool {
        match node {
            EndpointNode::Input => !self.inputs.contains(r),
            EndpointNode::Output => !self.outputs.contains(r),
            EndpointNode::PathOutput(leaf) => self.leaf_output_enabled(leaf, r),
        }
    }

    /// Whether the output node of Y leaf `leaf` keeps the endpoint `r`.
    pub fn leaf_output_enabled(&self, leaf: &PathRef, r: &EndpointRef) -> bool {
        self.path_outputs
            .iter()
            .find(|entry| entry.split == leaf.split && entry.path == leaf.path)
            .is_none_or(|entry| !entry.disabled.contains(r))
    }

    /// Check (`enabled`) or uncheck one endpoint on one node. Idempotent.
    pub fn set_enabled(&mut self, node: &EndpointNode, r: EndpointRef, enabled: bool) {
        let list = match node {
            EndpointNode::Input => &mut self.inputs,
            EndpointNode::Output => &mut self.outputs,
            EndpointNode::PathOutput(leaf) => self.leaf_list_mut(leaf),
        };
        list.retain(|kept| *kept != r);
        if !enabled {
            list.push(r);
        }
        self.path_outputs.retain(|entry| !entry.disabled.is_empty());
    }

    /// Whether a chain TAIL output stays live. A chain with Y leaves has no
    /// chain output node: the output lives while any leaf's node keeps it.
    pub fn tail_output_enabled(&self, leaves: &[PathRef], r: &EndpointRef) -> bool {
        if leaves.is_empty() {
            self.is_enabled(&EndpointNode::Output, r)
        } else {
            leaves.iter().any(|leaf| self.leaf_output_enabled(leaf, r))
        }
    }

    /// Drop the refs to endpoints the chain's bindings no longer offer (spec
    /// §1.3: ignored at runtime, dropped on the next save). `inputs` and
    /// `outputs` are what [`crate::endpoint_candidates::endpoint_candidates`]
    /// lists for the chain.
    pub fn retain_known(&mut self, inputs: &[EndpointRef], outputs: &[EndpointRef]) {
        self.inputs.retain(|r| inputs.contains(r));
        self.outputs.retain(|r| outputs.contains(r));
        for entry in &mut self.path_outputs {
            entry.disabled.retain(|r| outputs.contains(r));
        }
        self.path_outputs.retain(|entry| !entry.disabled.is_empty());
    }

    /// Forget the leaf output nodes of the splits in `gone` (removed, or no
    /// longer a Y). Only those: the checklists are the rig input's, so the
    /// leaves of a Y another preset holds stay.
    pub fn forget_splits(&mut self, gone: &[BlockId]) {
        self.path_outputs
            .retain(|entry| !gone.contains(&entry.split));
    }

    /// Forget the output node of path `removed` of `split` and shift the
    /// nodes of the paths above it down one, as the path list just did.
    pub fn shift_after_path_removed(&mut self, split: &BlockId, removed: usize) {
        self.path_outputs
            .retain(|entry| !(entry.split == *split && entry.path == removed));
        for entry in &mut self.path_outputs {
            if entry.split == *split && entry.path > removed {
                entry.path -= 1;
            }
        }
    }

    /// Move the pre-§11 `path_a_outputs`/`path_b_outputs` onto paths 0 and 1
    /// of the first top-level Y of `blocks`. Without a Y they are dropped.
    pub fn adopt_legacy_paths(&mut self, blocks: &[AudioBlock]) {
        let legacy = [
            std::mem::take(&mut self.legacy_path_a_outputs),
            std::mem::take(&mut self.legacy_path_b_outputs),
        ];
        let Some(position) = find_split_with_end(blocks, SplitEnd::Y).map(|(at, _)| at) else {
            return;
        };
        let split = blocks[position].id.clone();
        for (path, refs) in legacy.into_iter().enumerate() {
            let leaf = PathRef {
                split: split.clone(),
                path,
            };
            for r in refs {
                self.set_enabled(&EndpointNode::PathOutput(leaf.clone()), r, false);
            }
        }
    }

    fn leaf_list_mut(&mut self, leaf: &PathRef) -> &mut Vec<EndpointRef> {
        let at = match self
            .path_outputs
            .iter()
            .position(|entry| entry.split == leaf.split && entry.path == leaf.path)
        {
            Some(at) => at,
            None => {
                self.path_outputs.push(PathOutputDisables {
                    split: leaf.split.clone(),
                    path: leaf.path,
                    disabled: Vec::new(),
                });
                self.path_outputs.len() - 1
            }
        };
        &mut self.path_outputs[at].disabled
    }
}
