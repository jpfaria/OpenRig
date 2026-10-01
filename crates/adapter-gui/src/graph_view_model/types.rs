//! Responsibility: describes the graph the `GraphView` component renders
//!
//! Pure Rust types decoupled from Slint. Wiring code converts these into
//! Slint-generated structs before pushing to the UI.
//!
//! The component itself is **fully generic**: it knows nothing about amps,
//! drives, or signal-chain semantics. It only renders nodes at given
//! coordinates and edges between them.

/// Visual category of a node — used by the UI to colour the node panel.
///
/// Adding a new category does NOT require touching the component; the
/// UI layer (visual config) maps category → colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeCategory {
    /// Input source (mono/stereo/dual-mono).
    Input,
    /// Output sink.
    Output,
    /// Gate/compressor/dynamics.
    Dynamics,
    /// Drive/distortion/overdrive.
    Drive,
    /// Amplifier / preamp / cabinet.
    Amp,
    /// Modulation (chorus, phaser, flanger).
    Modulation,
    /// Time-based effects (delay, echo).
    Time,
    /// Reverb (room, hall, shimmer, plate).
    Reverb,
    /// Equalizer / filter.
    Eq,
    /// Utility (volume, splitter, merger, send).
    Util,
    /// Anything else.
    Other,
}

impl NodeCategory {
    /// Stable string identifier suitable for serialisation and for the
    /// Slint side to look up colours. Lower-case, no spaces.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
            Self::Dynamics => "dynamics",
            Self::Drive => "drive",
            Self::Amp => "amp",
            Self::Modulation => "modulation",
            Self::Time => "time",
            Self::Reverb => "reverb",
            Self::Eq => "eq",
            Self::Util => "util",
            Self::Other => "other",
        }
    }
}

/// What a node IS in the chain editor — picks the card the UI draws
/// (#328). Distinct from [`NodeCategory`], which only picks a colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NodeKind {
    /// A processing block.
    #[default]
    Block,
    /// The chain's input node.
    IoInput,
    /// A chain output node (one per lane in a fan-out).
    IoOutput,
    /// Where the signal becomes parallel lanes.
    Split,
    /// Where parallel lanes are summed back into one.
    Mixer,
}

impl NodeKind {
    /// Slug the Slint `GraphNode.kind` field carries.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::IoInput => "io_input",
            Self::IoOutput => "io_output",
            Self::Split => "split",
            Self::Mixer => "mixer",
        }
    }
}

/// A node in the graph view. Coordinates are in **layout space**
/// (logical pixels before zoom/pan). The component applies the
/// viewport transform when rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphNode {
    /// Stable identifier — must be unique within the graph. Used by edges
    /// and by callbacks (`node-clicked(id)`).
    pub id: String,
    /// Human-readable label shown on the node.
    pub label: String,
    /// Visual category — drives node colour.
    pub category: NodeCategory,
    /// What the node is — picks its card on the Slint side.
    pub kind: NodeKind,
    /// X position in layout space.
    pub x: f32,
    /// Y position in layout space.
    pub y: f32,
    /// Whether the node represents a bypassed block. Dim in the UI.
    pub bypass: bool,
}

/// An edge between two nodes — represents signal flow.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphEdge {
    /// Source node id.
    pub from_id: String,
    /// Target node id.
    pub to_id: String,
}

/// Logical stage of a signal chain. The layout helpers consume a
/// sequence of stages and produce positioned [`GraphNode`]s and
/// [`GraphEdge`]s.
#[derive(Debug, Clone, PartialEq)]
pub enum ChainStage {
    /// A single block — sits alone in one column.
    Single(BlockBlueprint),
    /// Parallel lanes after an auto-generated split node. Each inner `Vec`
    /// is one lane, top to bottom, and may hold further parallel stages at
    /// any depth; `end` decides whether the lanes merge again or fan out to
    /// one terminal each (#328). `split_id` names the split and merge nodes.
    Parallel {
        split_id: String,
        lanes: Vec<Vec<ChainStage>>,
        end: ParallelEnd,
    },
}

/// How a [`ChainStage::Parallel`] ends (#328).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParallelEnd {
    /// The lanes meet again at an auto-generated merge node and the next
    /// stage continues from it (Split → Mix).
    Merge,
    /// No merge node: each lane's last blueprint is its terminal — a Y
    /// chain's output node — and nothing may follow (Y → A/B).
    Fan,
}

/// Logical description of one block, without position. Position is
/// assigned by the chain builder.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockBlueprint {
    pub id: String,
    pub label: String,
    pub category: NodeCategory,
    pub bypass: bool,
    /// What the node is. [`BlockBlueprint::new`] makes a block; the host
    /// marks its I/O nodes with [`BlockBlueprint::with_kind`].
    pub kind: NodeKind,
}

impl BlockBlueprint {
    pub fn new(id: impl Into<String>, label: impl Into<String>, category: NodeCategory) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            category,
            bypass: false,
            kind: NodeKind::Block,
        }
    }

    /// The same blueprint as another kind of node (#328).
    pub fn with_kind(mut self, kind: NodeKind) -> Self {
        self.kind = kind;
        self
    }
}

/// Grid metrics shared by every layout path.
#[derive(Debug, Clone, Copy)]
pub struct GridMetrics {
    /// Horizontal distance between adjacent columns (centre to centre).
    pub column_spacing: f32,
    /// Vertical distance between parallel lanes (centre to centre).
    pub lane_spacing: f32,
    /// X coordinate of the first column's centre.
    pub origin_x: f32,
    /// Y coordinate of the central lane (single-path / merged blocks).
    pub origin_y: f32,
}

impl Default for GridMetrics {
    fn default() -> Self {
        Self {
            column_spacing: 160.0,
            lane_spacing: 120.0,
            origin_x: 80.0,
            origin_y: 200.0,
        }
    }
}
