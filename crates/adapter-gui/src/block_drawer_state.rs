//! Responsibility: says what the block drawer should be showing.

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
