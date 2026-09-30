//! Responsibility: decides whether a VST3 parameter is a user-facing control.
//!
//! Reads only the flags and names the plugin itself reports through its
//! `IEditController`; OpenRig never hardcodes any plugin (#1011).

use vst3_host::param_flags::{IS_BYPASS, IS_HIDDEN, IS_READ_ONLY};
use vst3_host::Vst3ParamInfo;

/// Placeholder names a plugin gives to slots that are not real controls:
/// reserved ids and unmapped macro slots (`Assign 1`, which does nothing until
/// mapped in the plugin's own editor). Some plugins still flag these
/// automatable, so the name alone decides.
const PLACEHOLDER_NAMES: &[&str] = &["reserved", "unused", "unnamed", "assign"];

/// `false` for parameters the plugin asks hosts not to show (`kIsHidden`),
/// output-only readouts (`kIsReadOnly`), the plugin's own bypass (`kIsBypass`,
/// duplicated by the block footswitch), and placeholder slots.
pub(crate) fn is_user_facing(param: &Vst3ParamInfo) -> bool {
    if param.flags & (IS_HIDDEN | IS_READ_ONLY | IS_BYPASS) != 0 {
        return false;
    }
    !(is_placeholder(&param.title) || is_placeholder(&param.short_title))
}

/// `RESERVED1`, `Reserved 2`, `unused_3` … : a placeholder word followed only
/// by separators and a slot number.
fn is_placeholder(name: &str) -> bool {
    let squashed: String = name
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_' && *c != '-')
        .collect::<String>()
        .to_lowercase();
    let stem = squashed.trim_end_matches(|c: char| c.is_ascii_digit());
    PLACEHOLDER_NAMES.contains(&stem)
}

#[cfg(test)]
#[path = "vst3_param_filter_tests.rs"]
mod tests;
