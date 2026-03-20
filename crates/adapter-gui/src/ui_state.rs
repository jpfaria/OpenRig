use project::track::Track;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageTypeDefinition {
    pub effect_type: &'static str,
    pub label: &'static str,
    pub icon_kind: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageModelDefinition {
    pub effect_type: &'static str,
    pub model_id: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub icon_kind: &'static str,
}

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

pub fn stage_types() -> Vec<StageTypeDefinition> {
    vec![
        StageTypeDefinition {
            effect_type: "amp_head",
            label: "Amp Head",
            icon_kind: "amp_head",
        },
        StageTypeDefinition {
            effect_type: "amp_combo",
            label: "Amp Combo",
            icon_kind: "amp_combo",
        },
        StageTypeDefinition {
            effect_type: "cab",
            label: "Cab",
            icon_kind: "cab",
        },
        StageTypeDefinition {
            effect_type: "full_rig",
            label: "Full Rig",
            icon_kind: "full_rig",
        },
        StageTypeDefinition {
            effect_type: "drive",
            label: "Drive",
            icon_kind: "drive",
        },
        StageTypeDefinition {
            effect_type: "compressor",
            label: "Compressor",
            icon_kind: "compressor",
        },
        StageTypeDefinition {
            effect_type: "gate",
            label: "Gate",
            icon_kind: "gate",
        },
        StageTypeDefinition {
            effect_type: "eq",
            label: "EQ",
            icon_kind: "eq",
        },
        StageTypeDefinition {
            effect_type: "tremolo",
            label: "Tremolo",
            icon_kind: "tremolo",
        },
        StageTypeDefinition {
            effect_type: "delay",
            label: "Delay",
            icon_kind: "delay",
        },
        StageTypeDefinition {
            effect_type: "reverb",
            label: "Reverb",
            icon_kind: "reverb",
        },
        StageTypeDefinition {
            effect_type: "tuner",
            label: "Tuner",
            icon_kind: "tuner",
        },
        StageTypeDefinition {
            effect_type: "nam",
            label: "NAM",
            icon_kind: "nam",
        },
    ]
}

pub fn stage_models_for_type(effect_type: &str) -> Vec<StageModelDefinition> {
    stage_model_catalog()
        .into_iter()
        .filter(|item| item.effect_type == effect_type)
        .collect()
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

fn stage_model_catalog() -> Vec<StageModelDefinition> {
    vec![
        StageModelDefinition {
            effect_type: "amp_head",
            model_id: "marshall_jcm_800_2203",
            title: "Marshall JCM 800 2203",
            subtitle: "Amp head classico de high gain",
            icon_kind: "amp_head",
        },
        StageModelDefinition {
            effect_type: "amp_combo",
            model_id: "bogner_ecstasy",
            title: "Bogner Ecstasy",
            subtitle: "Combo moderno com drive encorpado",
            icon_kind: "amp_combo",
        },
        StageModelDefinition {
            effect_type: "cab",
            model_id: "marshall_4x12_v30",
            title: "Marshall 4x12 V30",
            subtitle: "Caixa com IR selecionavel",
            icon_kind: "cab",
        },
        StageModelDefinition {
            effect_type: "full_rig",
            model_id: "roland_jc_120b_jazz_chorus",
            title: "Roland JC-120B Jazz Chorus",
            subtitle: "Rig completo limpo e espacial",
            icon_kind: "full_rig",
        },
        StageModelDefinition {
            effect_type: "drive",
            model_id: "blues_overdrive_bd_2",
            title: "Blues Overdrive BD-2",
            subtitle: "Overdrive articulado para base e lead",
            icon_kind: "drive",
        },
        StageModelDefinition {
            effect_type: "compressor",
            model_id: "compressor_studio_clean",
            title: "Studio Clean Compressor",
            subtitle: "Controle de dinamica transparente",
            icon_kind: "compressor",
        },
        StageModelDefinition {
            effect_type: "gate",
            model_id: "gate_basic",
            title: "Basic Gate",
            subtitle: "Noise gate simples e rapido",
            icon_kind: "gate",
        },
        StageModelDefinition {
            effect_type: "eq",
            model_id: "eq_three_band_basic",
            title: "Three Band Basic EQ",
            subtitle: "Equalizador de tres bandas",
            icon_kind: "eq",
        },
        StageModelDefinition {
            effect_type: "tremolo",
            model_id: "tremolo_sine",
            title: "Sine Tremolo",
            subtitle: "Modulacao ritmica suave",
            icon_kind: "tremolo",
        },
        StageModelDefinition {
            effect_type: "delay",
            model_id: "digital_clean",
            title: "Digital Clean",
            subtitle: "Delay digital limpo e direto",
            icon_kind: "delay",
        },
        StageModelDefinition {
            effect_type: "delay",
            model_id: "analog_warm",
            title: "Analog Warm",
            subtitle: "Delay analogico mais quente",
            icon_kind: "delay",
        },
        StageModelDefinition {
            effect_type: "delay",
            model_id: "tape_vintage",
            title: "Tape Vintage",
            subtitle: "Eco com caracter de fita",
            icon_kind: "delay",
        },
        StageModelDefinition {
            effect_type: "delay",
            model_id: "reverse",
            title: "Reverse",
            subtitle: "Delay reverso atmosferico",
            icon_kind: "delay",
        },
        StageModelDefinition {
            effect_type: "delay",
            model_id: "slapback",
            title: "Slapback",
            subtitle: "Reflexo curto e percussivo",
            icon_kind: "delay",
        },
        StageModelDefinition {
            effect_type: "delay",
            model_id: "modulated_delay",
            title: "Modulated Delay",
            subtitle: "Delay com modulacao sutil",
            icon_kind: "delay",
        },
        StageModelDefinition {
            effect_type: "reverb",
            model_id: "plate_foundation",
            title: "Plate Foundation",
            subtitle: "Plate reverb equilibrado",
            icon_kind: "reverb",
        },
        StageModelDefinition {
            effect_type: "tuner",
            model_id: "tuner_chromatic",
            title: "Chromatic Tuner",
            subtitle: "Afinador cromatico utilitario",
            icon_kind: "tuner",
        },
        StageModelDefinition {
            effect_type: "nam",
            model_id: "neural_amp_modeler",
            title: "Generic NAM",
            subtitle: "Neural amp model generico",
            icon_kind: "nam",
        },
    ]
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
    use super::{
        insertion_slot_indices, stage_drawer_state, stage_models_for_type, stage_types,
        track_routing_summary, StageDrawerMode,
    };
    use domain::ids::{DeviceId, TrackId};
    use project::track::{Track, TrackOutputMixdown};

    #[test]
    fn stage_types_expose_expected_desktop_catalog() {
        let types = stage_types();

        assert!(types.iter().any(|item| item.effect_type == "amp_head"));
        assert!(types.iter().any(|item| item.effect_type == "delay"));
        assert!(types.iter().any(|item| item.effect_type == "nam"));
    }

    #[test]
    fn stage_models_are_filtered_by_type() {
        let delay_models = stage_models_for_type("delay");

        assert!(delay_models.iter().any(|item| item.model_id == "digital_clean"));
        assert!(delay_models.iter().any(|item| item.model_id == "modulated_delay"));
        assert!(delay_models.iter().all(|item| item.effect_type == "delay"));
    }

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
