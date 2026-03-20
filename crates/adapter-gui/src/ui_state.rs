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
        title: if matches!(mode, BlockDrawerMode::Edit) {
            "Editar block"
        } else {
            "Adicionar block"
        },
        confirm_label: if matches!(mode, BlockDrawerMode::Edit) {
            "Salvar"
        } else {
            "Adicionar"
        },
        effect_type: effect_type.to_string(),
        model_id: model_id.map(ToString::to_string),
    }
}

pub fn block_family_for_kind(kind: &str) -> &'static str {
    match kind {
        "amp_head" | "amp_combo" | "full_rig" | "nam" => "amp",
        "cab" => "cab",
        "ir" => "ir",
        "drive" => "drive",
        "dynamics" => "dynamics",
        "filter" => "filter",
        "wah" => "wah",
        "modulation" => "modulation",
        "delay" | "reverb" => "space",
        "utility" => "utility",
        _ => "utility",
    }
}

#[cfg(test)]
pub fn insertion_slot_indices(block_count: usize) -> Vec<usize> {
    (0..=block_count).collect()
}

pub fn chain_routing_summary(chain: &Chain) -> String {
    format!(
        "Entrada {} -> Saida {}",
        channels_label(&chain.input_channels),
        channels_label(&chain.output_channels),
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
    use project::chain::{Chain, ChainOutputMixdown};

    #[test]
    fn insertion_slots_cover_edges_and_between_positions() {
        assert_eq!(insertion_slot_indices(0), vec![0]);
        assert_eq!(insertion_slot_indices(3), vec![0, 1, 2, 3]);
    }

    #[test]
    fn block_drawer_labels_match_add_mode() {
        let state = block_drawer_state(None, "delay", Some("digital_clean"));

        assert_eq!(state.mode, BlockDrawerMode::Add);
        assert_eq!(state.title, "Adicionar block");
        assert_eq!(state.confirm_label, "Adicionar");
    }

    #[test]
    fn block_drawer_labels_match_edit_mode() {
        let state = block_drawer_state(Some(2), "delay", Some("digital_clean"));

        assert_eq!(state.mode, BlockDrawerMode::Edit);
        assert_eq!(state.title, "Editar block");
        assert_eq!(state.confirm_label, "Salvar");
    }

    #[test]
    fn routing_summary_uses_human_friendly_channel_numbers() {
        let chain = Chain {
            id: ChainId("chain:1".to_string()),
            description: Some("Guitarra".to_string()),
            enabled: true,
            input_device_id: DeviceId("in".to_string()),
            input_channels: vec![0],
            output_device_id: DeviceId("out".to_string()),
            output_channels: vec![0, 1],
            blocks: Vec::new(),
            output_mixdown: ChainOutputMixdown::Average,
        };

        assert_eq!(
            chain_routing_summary(&chain),
            "Entrada 1 -> Saida 1, 2".to_string()
        );
    }
}
