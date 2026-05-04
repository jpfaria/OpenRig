//! Per-widget `ParameterSpec` constructor functions. Each builder produces
//! a `ParameterSpec` pre-configured for one widget kind (knob / toggle /
//! file picker / select / text input / multi-slider / curve editor) so
//! per-effect crates declare their schema with a single call instead of
//! hand-assembling 9-field struct literals.
//!
//! Lifted out of `param.rs` (Phase 6 of issue #194). One responsibility:
//! ergonomic constructors for the schema layer.

use domain::value_objects::ParameterValue;

use super::schema::{
    CurveEditorRole, ParameterDomain, ParameterOption, ParameterSpec, ParameterUnit,
    ParameterWidget,
};

#[allow(clippy::too_many_arguments)]
pub fn float_parameter(
    path: &str,
    label: &str,
    group: Option<&str>,
    default_value: Option<f32>,
    min: f32,
    max: f32,
    step: f32,
    unit: ParameterUnit,
) -> ParameterSpec {
    ParameterSpec {
        path: path.to_string(),
        label: label.to_string(),
        group: group.map(ToString::to_string),
        widget: ParameterWidget::Knob,
        unit,
        domain: ParameterDomain::FloatRange { min, max, step },
        default_value: default_value.map(ParameterValue::Float),
        optional: false,
        allow_empty: false,
    }
}

pub fn bool_parameter(
    path: &str,
    label: &str,
    group: Option<&str>,
    default_value: Option<bool>,
) -> ParameterSpec {
    ParameterSpec {
        path: path.to_string(),
        label: label.to_string(),
        group: group.map(ToString::to_string),
        widget: ParameterWidget::Toggle,
        unit: ParameterUnit::None,
        domain: ParameterDomain::Bool,
        default_value: default_value.map(ParameterValue::Bool),
        optional: false,
        allow_empty: false,
    }
}

pub fn file_path_parameter(
    path: &str,
    label: &str,
    group: Option<&str>,
    default_value: Option<ParameterValue>,
    extensions: &[&str],
    optional: bool,
) -> ParameterSpec {
    ParameterSpec {
        path: path.to_string(),
        label: label.to_string(),
        group: group.map(ToString::to_string),
        widget: ParameterWidget::FilePicker,
        unit: ParameterUnit::None,
        domain: ParameterDomain::FilePath {
            extensions: extensions
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
        },
        default_value,
        optional,
        allow_empty: false,
    }
}

pub fn enum_parameter(
    path: &str,
    label: &str,
    group: Option<&str>,
    default_value: Option<&str>,
    options: &[(&str, &str)],
) -> ParameterSpec {
    ParameterSpec {
        path: path.to_string(),
        label: label.to_string(),
        group: group.map(ToString::to_string),
        widget: ParameterWidget::Select,
        unit: ParameterUnit::None,
        domain: ParameterDomain::Enum {
            options: options
                .iter()
                .map(|(value, option_label)| ParameterOption {
                    value: (*value).to_string(),
                    label: (*option_label).to_string(),
                })
                .collect(),
        },
        default_value: default_value.map(|value| ParameterValue::String(value.to_string())),
        optional: false,
        allow_empty: false,
    }
}

pub fn text_parameter(
    path: &str,
    label: &str,
    group: Option<&str>,
    default_value: Option<&str>,
    optional: bool,
) -> ParameterSpec {
    ParameterSpec {
        path: path.to_string(),
        label: label.to_string(),
        group: group.map(ToString::to_string),
        widget: ParameterWidget::TextInput,
        unit: ParameterUnit::None,
        domain: ParameterDomain::Text,
        default_value: default_value.map(|value| ParameterValue::String(value.to_string())),
        optional,
        allow_empty: false,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn multi_slider_parameter(
    path: &str,
    label: &str,
    group: Option<&str>,
    default_value: Option<f32>,
    min: f32,
    max: f32,
    step: f32,
    unit: ParameterUnit,
) -> ParameterSpec {
    ParameterSpec {
        path: path.to_string(),
        label: label.to_string(),
        group: group.map(ToString::to_string),
        widget: ParameterWidget::MultiSlider,
        unit,
        domain: ParameterDomain::FloatRange { min, max, step },
        default_value: default_value.map(ParameterValue::Float),
        optional: false,
        allow_empty: false,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn curve_editor_parameter(
    path: &str,
    label: &str,
    group: Option<&str>,
    role: CurveEditorRole,
    default_value: Option<f32>,
    min: f32,
    max: f32,
    step: f32,
    unit: ParameterUnit,
) -> ParameterSpec {
    ParameterSpec {
        path: path.to_string(),
        label: label.to_string(),
        group: group.map(ToString::to_string),
        widget: ParameterWidget::CurveEditor { role },
        unit,
        domain: ParameterDomain::FloatRange { min, max, step },
        default_value: default_value.map(ParameterValue::Float),
        optional: false,
        allow_empty: false,
    }
}
