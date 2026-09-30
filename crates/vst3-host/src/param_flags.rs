//! Responsibility: names the VST3 `ParameterInfo.flags` bits OpenRig reads.
//!
//! Re-exported from the SDK bindings so crates that only see `Vst3ParamInfo`
//! (e.g. the schema builder) can test the bits without depending on `vst3`.

use vst3::Steinberg::Vst::ParameterInfo_::ParameterFlags_;

/// The parameter can be automated by the host (real, user-facing control).
pub const CAN_AUTOMATE: i32 = ParameterFlags_::kCanAutomate;
/// The parameter is output-only (meters, readouts); it cannot be edited.
pub const IS_READ_ONLY: i32 = ParameterFlags_::kIsReadOnly;
/// The plugin asks hosts not to show this parameter.
pub const IS_HIDDEN: i32 = ParameterFlags_::kIsHidden;
/// The plugin's own bypass switch (OpenRig's block footswitch already is one).
pub const IS_BYPASS: i32 = ParameterFlags_::kIsBypass;
