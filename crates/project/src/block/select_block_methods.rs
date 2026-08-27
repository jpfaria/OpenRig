//! Responsibility: answers which option a select block is on.

use super::types::{AudioBlock, AudioBlockKind, SelectBlock, MAX_SELECT_OPTIONS};

impl SelectBlock {
    pub fn selected_option(&self) -> Option<&AudioBlock> {
        self.options
            .iter()
            .find(|option| option.id == self.selected_block_id)
    }

    pub fn validate_structure(&self) -> Result<(), String> {
        if self.options.is_empty() {
            return Err("select block must define at least one option".to_string());
        }
        if self.options.len() > MAX_SELECT_OPTIONS {
            return Err(format!(
                "select block may define up to {} options",
                MAX_SELECT_OPTIONS
            ));
        }

        let mut effect_type = None::<&str>;
        for option in &self.options {
            if matches!(
                option.kind,
                AudioBlockKind::Select(_)
                    | AudioBlockKind::Input(_)
                    | AudioBlockKind::Output(_)
                    | AudioBlockKind::Insert(_)
            ) {
                return Err(
                    "select block options cannot be select, input, output, or insert blocks"
                        .to_string(),
                );
            }

            let model = option.model_ref().ok_or_else(|| {
                format!(
                    "select block option '{}' does not expose a concrete model",
                    option.id.0
                )
            })?;

            match effect_type {
                Some(existing) if existing != model.effect_type => {
                    return Err("select block options must use the same effect type".to_string());
                }
                None => effect_type = Some(model.effect_type),
                _ => {}
            }
        }

        if self.selected_option().is_none() {
            return Err("select block selected option does not exist".to_string());
        }

        Ok(())
    }
}
