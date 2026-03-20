use project::track::Track;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageDrawerMode {
    Add,
    Edit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageDrawerState {
    pub mode: StageDrawerMode,
    pub title: &'static str,
    pub confirm_label: &'static str,
    pub effect_type: String,
    pub model_id: Option<String>,
}

pub fn stage_drawer_state(
    stage_index: Option<usize>,
    effect_type: &str,
    model_id: Option<&str>,
) -> StageDrawerState {
    let mode = if stage_index.is_some() {
        StageDrawerMode::Edit
    } else {
        StageDrawerMode::Add
    };

    StageDrawerState {
        mode,
        title: if matches!(mode, StageDrawerMode::Edit) {
            "Editar stage"
        } else {
            "Adicionar stage"
        },
        confirm_label: if matches!(mode, StageDrawerMode::Edit) {
            "Salvar"
        } else {
            "Adicionar"
        },
        effect_type: effect_type.to_string(),
        model_id: model_id.map(ToString::to_string),
    }
}

pub fn stage_family_for_kind(kind: &str) -> &'static str {
    match kind {
        "amp_head" | "amp_combo" | "full_rig" | "nam" => "amp",
        "drive" => "gain",
        "compressor" | "gate" => "dynamics",
        "eq" => "filter",
        "tremolo" => "modulation",
        "delay" | "reverb" => "space",
        "tuner" => "utility",
        _ => "utility",
    }
}

#[cfg(test)]
pub fn insertion_slot_indices(stage_count: usize) -> Vec<usize> {
    (0..=stage_count).collect()
}

pub fn track_routing_summary(track: &Track) -> String {
    format!(
        "Entrada {} -> Saida {}",
        channels_label(&track.input_channels),
        channels_label(&track.output_channels),
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
    use super::{insertion_slot_indices, stage_drawer_state, track_routing_summary, StageDrawerMode};
    use domain::ids::{DeviceId, TrackId};
    use project::track::{Track, TrackOutputMixdown};

    #[test]
    fn insertion_slots_cover_edges_and_between_positions() {
        assert_eq!(insertion_slot_indices(0), vec![0]);
        assert_eq!(insertion_slot_indices(3), vec![0, 1, 2, 3]);
    }

    #[test]
    fn stage_drawer_labels_match_add_mode() {
        let state = stage_drawer_state(None, "delay", Some("digital_clean"));

        assert_eq!(state.mode, StageDrawerMode::Add);
        assert_eq!(state.title, "Adicionar stage");
        assert_eq!(state.confirm_label, "Adicionar");
    }

    #[test]
    fn stage_drawer_labels_match_edit_mode() {
        let state = stage_drawer_state(Some(2), "delay", Some("digital_clean"));

        assert_eq!(state.mode, StageDrawerMode::Edit);
        assert_eq!(state.title, "Editar stage");
        assert_eq!(state.confirm_label, "Salvar");
    }

    #[test]
    fn routing_summary_uses_human_friendly_channel_numbers() {
        let track = Track {
            id: TrackId("track:1".to_string()),
            description: Some("Guitarra".to_string()),
            enabled: true,
            input_device_id: DeviceId("in".to_string()),
            input_channels: vec![0],
            output_device_id: DeviceId("out".to_string()),
            output_channels: vec![0, 1],
            blocks: Vec::new(),
            output_mixdown: TrackOutputMixdown::Average,
        };

        assert_eq!(
            track_routing_summary(&track),
            "Entrada 1 -> Saida 1, 2".to_string()
        );
    }
}
