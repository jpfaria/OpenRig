use project::chain::Chain;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockDrawerMode {
    Add,
    Edit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockDrawerState {
    pub mode: BlockDrawerMode,
    pub title: &'static str,
    pub confirm_label: &'static str,
    pub effect_type: String,
    pub model_id: Option<String>,
}

pub fn block_drawer_state(
    block_index: Option<usize>,
    effect_type: &str,
    model_id: Option<&str>,
) -> BlockDrawerState {
    let mode = if block_index.is_some() {
        BlockDrawerMode::Edit
    } else {
        BlockDrawerMode::Add
    };

    BlockDrawerState {
        mode,
        title: "",
        confirm_label: if matches!(mode, BlockDrawerMode::Edit) {
            "Salvar"
        } else {
            "Adicionar"
        },
        effect_type: effect_type.to_string(),
        model_id: model_id.map(ToString::to_string),
    }
}

/// Returns the accent color (RGBA) for an effect type icon_kind.
/// Single source of truth — used by all UI components.
pub fn accent_color_for_icon_kind(icon_kind: &str) -> slint::Color {
    match icon_kind {
        "preamp" => slint::Color::from_argb_u8(255, 0xf2, 0x9f, 0x38),
        "amp" => slint::Color::from_argb_u8(255, 0xf0, 0x62, 0x92),
        "cab" => slint::Color::from_argb_u8(255, 0xf2, 0xcb, 0x54),
        "body" => slint::Color::from_argb_u8(255, 0xc8, 0x94, 0x6a),
        "ir" => slint::Color::from_argb_u8(255, 0x7c, 0xc9, 0xff),
        "full_rig" => slint::Color::from_argb_u8(255, 0x63, 0xd2, 0xff),
        "gain" => slint::Color::from_argb_u8(255, 0xff, 0x6a, 0x57),
        "dynamics" => slint::Color::from_argb_u8(255, 0x41, 0xb8, 0xff),
        "filter" => slint::Color::from_argb_u8(255, 0xd6, 0xc8, 0x5a),
        "wah" => slint::Color::from_argb_u8(255, 0x78, 0xd0, 0x6b),
        "modulation" => slint::Color::from_argb_u8(255, 0x58, 0xd3, 0x9b),
        "delay" => slint::Color::from_argb_u8(255, 0xba, 0x8c, 0xff),
        "reverb" => slint::Color::from_argb_u8(255, 0x6d, 0xe1, 0xd2),
        "utility" => slint::Color::from_argb_u8(255, 0x95, 0xa0, 0xb2),
        "nam" => slint::Color::from_argb_u8(255, 0xff, 0x7c, 0xd7),
        "pitch" => slint::Color::from_argb_u8(255, 0x8f, 0x8c, 0xff),
        "insert" => slint::Color::from_argb_u8(255, 0xf2, 0x9f, 0x38),
        "input" => slint::Color::from_argb_u8(255, 0x45, 0xa7, 0xff),
        "output" => slint::Color::from_argb_u8(255, 0x45, 0xa7, 0xff),
        _ => slint::Color::from_argb_u8(255, 0x7f, 0xb0, 0xff),
    }
}

/// Icon SVG index for an icon_kind. Used to load the correct icon from a pre-built array.
/// Returns a numeric index into EFFECT_TYPE_ICONS.
#[allow(dead_code)]
pub fn icon_index_for_icon_kind(icon_kind: &str) -> usize {
    match icon_kind {
        "preamp" => 0,
        "amp" => 1,
        "cab" => 2,
        "body" => 3,
        "ir" => 4,
        "full_rig" => 5,
        "gain" => 6,
        "dynamics" => 7,
        "filter" => 8,
        "wah" => 9,
        "modulation" => 10,
        "delay" => 11,
        "reverb" => 12,
        "utility" => 13,
        "nam" => 14,
        "pitch" => 15,
        _ => 13, // utility fallback
    }
}

pub fn block_family_for_kind(kind: &str) -> &'static str {
    use block_core::*;
    match kind {
        EFFECT_TYPE_PREAMP | EFFECT_TYPE_AMP | EFFECT_TYPE_FULL_RIG | EFFECT_TYPE_NAM => "amp",
        EFFECT_TYPE_CAB => "cab",
        EFFECT_TYPE_BODY => "body",
        EFFECT_TYPE_IR => "ir",
        EFFECT_TYPE_GAIN => "gain",
        EFFECT_TYPE_DYNAMICS => "dynamics",
        EFFECT_TYPE_FILTER => "filter",
        EFFECT_TYPE_WAH => "wah",
        EFFECT_TYPE_PITCH => "pitch",
        EFFECT_TYPE_MODULATION => "modulation",
        EFFECT_TYPE_DELAY | EFFECT_TYPE_REVERB => "space",
        EFFECT_TYPE_UTILITY => "utility",
        "input" | "output" | "insert" => "routing",
        _ => "utility",
    }
}

#[cfg(test)]
pub fn insertion_slot_indices(block_count: usize) -> Vec<usize> {
    (0..=block_count).collect()
}

pub fn chain_routing_summary(chain: &Chain) -> String {
    let input_channels: Vec<usize> = chain.input_blocks().into_iter()
        .flat_map(|(_, ib)| ib.entries.iter().flat_map(|e| e.channels.iter().copied()))
        .collect();
    let output_channels: Vec<usize> = chain.output_blocks().into_iter()
        .flat_map(|(_, ob)| ob.entries.iter().flat_map(|e| e.channels.iter().copied()))
        .collect();
    format!(
        "Entrada {} -> Saida {}",
        channels_label(&input_channels),
        channels_label(&output_channels),
    )
}

fn channels_label(channels: &[usize]) -> String {
    channels
        .iter()
        .map(|channel| (channel + 1).to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::{insertion_slot_indices, block_drawer_state, chain_routing_summary, BlockDrawerMode};
    use domain::ids::{DeviceId, ChainId};
    use project::block::{AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry};
    use project::chain::{Chain, ChainInputMode, ChainOutputMode};

    #[test]
    fn insertion_slots_cover_edges_and_between_positions() {
        assert_eq!(insertion_slot_indices(0), vec![0]);
        assert_eq!(insertion_slot_indices(3), vec![0, 1, 2, 3]);
    }

    #[test]
    fn block_drawer_labels_match_add_mode() {
        let state = block_drawer_state(None, "delay", Some("digital_clean"));

        assert_eq!(state.mode, BlockDrawerMode::Add);
        assert_eq!(state.title, "");
        assert_eq!(state.confirm_label, "Adicionar");
    }

    #[test]
    fn block_drawer_labels_match_edit_mode() {
        let state = block_drawer_state(Some(2), "delay", Some("digital_clean"));

        assert_eq!(state.mode, BlockDrawerMode::Edit);
        assert_eq!(state.title, "");
        assert_eq!(state.confirm_label, "Salvar");
    }

    // --- accent_color_for_icon_kind ---

    use super::{accent_color_for_icon_kind, icon_index_for_icon_kind, block_family_for_kind};

    #[test]
    fn accent_color_returns_distinct_color_for_each_known_kind() {
        let kinds = [
            "preamp", "amp", "cab", "body", "ir", "full_rig", "gain",
            "dynamics", "filter", "wah", "modulation", "delay", "reverb",
            "utility", "nam", "pitch", "insert", "input", "output",
        ];
        for kind in &kinds {
            let color = accent_color_for_icon_kind(kind);
            assert_eq!(color.alpha(), 255, "alpha must be 255 for kind '{}'", kind);
        }
    }

    #[test]
    fn accent_color_returns_fallback_for_unknown_kind() {
        let fallback = accent_color_for_icon_kind("nonexistent_kind");
        let expected = slint::Color::from_argb_u8(255, 0x7f, 0xb0, 0xff);
        assert_eq!(fallback, expected);
    }

    #[test]
    fn accent_color_preamp_is_orange() {
        let c = accent_color_for_icon_kind("preamp");
        assert_eq!(c, slint::Color::from_argb_u8(255, 0xf2, 0x9f, 0x38));
    }

    #[test]
    fn accent_color_input_output_share_same_color() {
        assert_eq!(
            accent_color_for_icon_kind("input"),
            accent_color_for_icon_kind("output"),
        );
    }

    // --- icon_index_for_icon_kind ---

    #[test]
    fn icon_index_returns_correct_index_for_known_kinds() {
        assert_eq!(icon_index_for_icon_kind("preamp"), 0);
        assert_eq!(icon_index_for_icon_kind("amp"), 1);
        assert_eq!(icon_index_for_icon_kind("cab"), 2);
        assert_eq!(icon_index_for_icon_kind("body"), 3);
        assert_eq!(icon_index_for_icon_kind("ir"), 4);
        assert_eq!(icon_index_for_icon_kind("full_rig"), 5);
        assert_eq!(icon_index_for_icon_kind("gain"), 6);
        assert_eq!(icon_index_for_icon_kind("dynamics"), 7);
        assert_eq!(icon_index_for_icon_kind("filter"), 8);
        assert_eq!(icon_index_for_icon_kind("wah"), 9);
        assert_eq!(icon_index_for_icon_kind("modulation"), 10);
        assert_eq!(icon_index_for_icon_kind("delay"), 11);
        assert_eq!(icon_index_for_icon_kind("reverb"), 12);
        assert_eq!(icon_index_for_icon_kind("utility"), 13);
        assert_eq!(icon_index_for_icon_kind("nam"), 14);
        assert_eq!(icon_index_for_icon_kind("pitch"), 15);
    }

    #[test]
    fn icon_index_unknown_falls_back_to_utility() {
        assert_eq!(icon_index_for_icon_kind("unknown"), 13);
        assert_eq!(icon_index_for_icon_kind(""), 13);
    }

    // --- block_family_for_kind ---

    #[test]
    fn block_family_groups_amp_related_kinds() {
        assert_eq!(block_family_for_kind("preamp"), "amp");
        assert_eq!(block_family_for_kind("amp"), "amp");
        assert_eq!(block_family_for_kind("full_rig"), "amp");
        assert_eq!(block_family_for_kind("nam"), "amp");
    }

    #[test]
    fn block_family_groups_space_kinds() {
        assert_eq!(block_family_for_kind("delay"), "space");
        assert_eq!(block_family_for_kind("reverb"), "space");
    }

    #[test]
    fn block_family_groups_routing_kinds() {
        assert_eq!(block_family_for_kind("input"), "routing");
        assert_eq!(block_family_for_kind("output"), "routing");
        assert_eq!(block_family_for_kind("insert"), "routing");
    }

    #[test]
    fn block_family_returns_individual_families() {
        assert_eq!(block_family_for_kind("cab"), "cab");
        assert_eq!(block_family_for_kind("body"), "body");
        assert_eq!(block_family_for_kind("ir"), "ir");
        assert_eq!(block_family_for_kind("gain"), "gain");
        assert_eq!(block_family_for_kind("dynamics"), "dynamics");
        assert_eq!(block_family_for_kind("filter"), "filter");
        assert_eq!(block_family_for_kind("wah"), "wah");
        assert_eq!(block_family_for_kind("pitch"), "pitch");
        assert_eq!(block_family_for_kind("modulation"), "modulation");
        assert_eq!(block_family_for_kind("utility"), "utility");
    }

    #[test]
    fn block_family_unknown_falls_back_to_utility() {
        assert_eq!(block_family_for_kind("unknown_kind"), "utility");
        assert_eq!(block_family_for_kind(""), "utility");
    }

    // --- block_drawer_state edge cases ---

    #[test]
    fn block_drawer_state_add_mode_without_model_id() {
        let state = block_drawer_state(None, "reverb", None);
        assert_eq!(state.mode, BlockDrawerMode::Add);
        assert_eq!(state.effect_type, "reverb");
        assert!(state.model_id.is_none());
    }

    #[test]
    fn block_drawer_state_edit_mode_preserves_model_id() {
        let state = block_drawer_state(Some(0), "gain", Some("ts9"));
        assert_eq!(state.mode, BlockDrawerMode::Edit);
        assert_eq!(state.model_id, Some("ts9".to_string()));
    }

    #[test]
    fn routing_summary_uses_human_friendly_channel_numbers() {
        use domain::ids::BlockId;
        let chain = Chain {
            id: ChainId("chain:1".to_string()),
            description: Some("Guitarra".to_string()),
            instrument: block_core::INST_ELECTRIC_GUITAR.to_string(),
            enabled: true,
            blocks: vec![
                AudioBlock {
                    id: BlockId("chain:1:input:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".to_string(),
                        entries: vec![InputEntry {
                            name: "Input 1".to_string(),
                            device_id: DeviceId("in".to_string()),
                            mode: ChainInputMode::Mono,
                            channels: vec![0],
                        }],
                    }),
                },
                AudioBlock {
                    id: BlockId("chain:1:output:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: "standard".to_string(),
                        entries: vec![OutputEntry {
                            name: "Output 1".to_string(),
                            device_id: DeviceId("out".to_string()),
                            mode: ChainOutputMode::Stereo,
                            channels: vec![0, 1],
                        }],
                    }),
                },
            ],
        };

        assert_eq!(
            chain_routing_summary(&chain),
            "Entrada 1 -> Saida 1, 2".to_string()
        );
    }
}
