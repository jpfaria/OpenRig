//! Responsibility: records which endpoints of a chain's own bindings each graph node leaves out.
//!
//! #328 (spec §1.3): the input and output nodes of a chain's graph list every
//! endpoint of the chain's E/S bindings, checked by default. Unchecking one
//! leaves it out of THAT node only — it stays listed, and nothing is removed
//! from the E/S. This is chain configuration, not preset data (one preset is
//! reused by several inputs), so it lives on `RigInput` and on the projected
//! `Chain`.

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EndpointNode {
    /// The chain's input node.
    Input,
    /// The chain's output node (chains without a Y split).
    Output,
    /// Path A's output node of the chain's Y split.
    PathAOutput,
    /// Path B's output node of the chain's Y split.
    PathBOutput,
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
    pub path_a_outputs: Vec<EndpointRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_b_outputs: Vec<EndpointRef>,
}

impl EndpointDisables {
    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
            && self.outputs.is_empty()
            && self.path_a_outputs.is_empty()
            && self.path_b_outputs.is_empty()
    }

    /// Whether `node` keeps the endpoint `r` (checked).
    pub fn is_enabled(&self, node: EndpointNode, r: &EndpointRef) -> bool {
        !self.list(node).contains(r)
    }

    /// Check (`enabled`) or uncheck one endpoint on one node. Idempotent.
    pub fn set_enabled(&mut self, node: EndpointNode, r: EndpointRef, enabled: bool) {
        let list = self.list_mut(node);
        list.retain(|kept| *kept != r);
        if !enabled {
            list.push(r);
        }
    }

    /// Whether a chain TAIL output stays live. A Y → A/B chain has no chain
    /// output node: the output lives while either path's node keeps it.
    pub fn tail_output_enabled(&self, y_split: bool, r: &EndpointRef) -> bool {
        if y_split {
            self.is_enabled(EndpointNode::PathAOutput, r)
                || self.is_enabled(EndpointNode::PathBOutput, r)
        } else {
            self.is_enabled(EndpointNode::Output, r)
        }
    }

    /// Drop the refs to endpoints the chain's bindings no longer offer (spec
    /// §1.3: ignored at runtime, dropped on the next save). `inputs` and
    /// `outputs` are what [`crate::endpoint_candidates::endpoint_candidates`]
    /// lists for the chain.
    pub fn retain_known(&mut self, inputs: &[EndpointRef], outputs: &[EndpointRef]) {
        self.inputs.retain(|r| inputs.contains(r));
        self.outputs.retain(|r| outputs.contains(r));
        self.path_a_outputs.retain(|r| outputs.contains(r));
        self.path_b_outputs.retain(|r| outputs.contains(r));
    }

    fn list(&self, node: EndpointNode) -> &Vec<EndpointRef> {
        match node {
            EndpointNode::Input => &self.inputs,
            EndpointNode::Output => &self.outputs,
            EndpointNode::PathAOutput => &self.path_a_outputs,
            EndpointNode::PathBOutput => &self.path_b_outputs,
        }
    }

    fn list_mut(&mut self, node: EndpointNode) -> &mut Vec<EndpointRef> {
        match node {
            EndpointNode::Input => &mut self.inputs,
            EndpointNode::Output => &mut self.outputs,
            EndpointNode::PathAOutput => &mut self.path_a_outputs,
            EndpointNode::PathBOutput => &mut self.path_b_outputs,
        }
    }
}
