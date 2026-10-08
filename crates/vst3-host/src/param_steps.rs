//! Responsibility: decides whether a stepped VST3 parameter is shown as a selector.

/// The most steps a parameter can have and still be listed option by option.
pub const MAX_SELECTOR_STEPS: i32 = 128;

/// Whether a parameter with `step_count` steps is a selector. A parameter with
/// more steps (an integer range, which DPF reports one step per value) is a
/// knob: reading one label per step would stall the controller walk (#1104).
pub fn is_selector(step_count: i32) -> bool {
    (1..=MAX_SELECTOR_STEPS).contains(&step_count)
}

#[cfg(test)]
#[path = "param_steps_tests.rs"]
mod tests;
