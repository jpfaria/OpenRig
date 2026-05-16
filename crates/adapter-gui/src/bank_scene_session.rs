//! Pure bank/scene navigation core (#453).
//!
//! **No Slint here.** Slint is a pure dispatcher: a UI callback maps to a
//! [`BankSceneEvent`], [`BankSceneState::apply`] mutates the presentation
//! state and returns transport-agnostic [`BankSceneEffect`]s for the host to
//! execute (engine wiring is out of #453 scope). Fully unit-testable without
//! an `AppWindow` (CLAUDE.md / memory: no business logic in the screen).

use project::rig::RigProject;
use std::path::PathBuf;

/// Navigator state for one input: its (sorted, gap-preserving) bank slots and
/// the active preset slot + scene.
#[derive(Debug, Clone, PartialEq)]
pub struct InputNav {
    pub input: String,
    pub label: Option<String>,
    pub bank_slots: Vec<usize>,
    pub active_preset: usize,
    pub active_scene: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BankSceneState {
    pub project_open: bool,
    pub selected_input: Option<String>,
    pub inputs: Vec<InputNav>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BankSceneEvent {
    SelectInput(String),
    BankNext,
    BankPrev,
    SelectSlot(usize),
    SceneNext,
    ScenePrev,
    SelectScene(usize),
    OpenProject(PathBuf),
    CreateProject(PathBuf),
}

/// Transport-agnostic outcome the host applies (engine call, file open …).
#[derive(Debug, Clone, PartialEq)]
pub enum BankSceneEffect {
    SwitchPreset { input: String, slot: usize },
    SwitchScene { input: String, scene: usize },
    OpenProject(PathBuf),
    CreateProject(PathBuf),
}

impl BankSceneState {
    pub fn from_project(project: &RigProject) -> Self {
        let inputs: Vec<InputNav> = project
            .inputs
            .iter()
            .map(|(name, input)| InputNav {
                input: name.clone(),
                label: input.label.clone(),
                bank_slots: input.bank.keys().copied().collect(), // BTreeMap ⇒ sorted
                active_preset: input.active_preset,
                active_scene: input.active_scene,
            })
            .collect();
        let selected_input = inputs.first().map(|i| i.input.clone());
        Self {
            project_open: !inputs.is_empty(),
            selected_input,
            inputs,
        }
    }

    pub fn input(&self, name: &str) -> Option<&InputNav> {
        self.inputs.iter().find(|i| i.input == name)
    }

    fn selected_mut(&mut self) -> Option<&mut InputNav> {
        let sel = self.selected_input.clone()?;
        self.inputs.iter_mut().find(|i| i.input == sel)
    }

    pub fn apply(&mut self, event: BankSceneEvent) -> Vec<BankSceneEffect> {
        match event {
            BankSceneEvent::SelectInput(name) => {
                if self.inputs.iter().any(|i| i.input == name) {
                    self.selected_input = Some(name);
                }
                Vec::new()
            }
            BankSceneEvent::BankNext | BankSceneEvent::BankPrev => {
                let next = matches!(event, BankSceneEvent::BankNext);
                let Some(nav) = self.selected_mut() else {
                    return Vec::new();
                };
                let Some(pos) = nav.bank_slots.iter().position(|&s| s == nav.active_preset) else {
                    return Vec::new();
                };
                let new_pos = if next {
                    (pos + 1).min(nav.bank_slots.len().saturating_sub(1))
                } else {
                    pos.saturating_sub(1)
                };
                if new_pos == pos {
                    return Vec::new(); // clamped: no change, no effect
                }
                let slot = nav.bank_slots[new_pos];
                nav.active_preset = slot;
                let input = nav.input.clone();
                vec![BankSceneEffect::SwitchPreset { input, slot }]
            }
            BankSceneEvent::SelectSlot(slot) => {
                let Some(nav) = self.selected_mut() else {
                    return Vec::new();
                };
                if !nav.bank_slots.contains(&slot) || nav.active_preset == slot {
                    return Vec::new();
                }
                nav.active_preset = slot;
                let input = nav.input.clone();
                vec![BankSceneEffect::SwitchPreset { input, slot }]
            }
            BankSceneEvent::SceneNext | BankSceneEvent::ScenePrev => {
                let next = matches!(event, BankSceneEvent::SceneNext);
                let Some(nav) = self.selected_mut() else {
                    return Vec::new();
                };
                let target = if next {
                    nav.active_scene + 1
                } else {
                    nav.active_scene.saturating_sub(1)
                };
                if !(1..=8).contains(&target) || target == nav.active_scene {
                    return Vec::new();
                }
                nav.active_scene = target;
                let input = nav.input.clone();
                vec![BankSceneEffect::SwitchScene {
                    input,
                    scene: target,
                }]
            }
            BankSceneEvent::SelectScene(scene) => {
                let Some(nav) = self.selected_mut() else {
                    return Vec::new();
                };
                if !(1..=8).contains(&scene) || nav.active_scene == scene {
                    return Vec::new();
                }
                nav.active_scene = scene;
                let input = nav.input.clone();
                vec![BankSceneEffect::SwitchScene { input, scene }]
            }
            BankSceneEvent::OpenProject(p) => {
                vec![BankSceneEffect::OpenProject(p)]
            }
            BankSceneEvent::CreateProject(p) => {
                vec![BankSceneEffect::CreateProject(p)]
            }
        }
    }
}

#[cfg(test)]
#[path = "bank_scene_session_tests.rs"]
mod tests;
