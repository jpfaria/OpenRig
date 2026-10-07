//! #879: the capture names of a TONE3000 tone become the plugin's selectable
//! axes, by the same rules the curated OpenRig-plugins manifests follow:
//! constant words drop, knob settings become numeric knobs, mic / position /
//! speaker / voicing words become choices, and whatever is left names a preset.

use std::path::PathBuf;

use application::tone3000::api_types::{Model, Page};
use application::tone3000::axes::{infer_axes, Axes, CaptureKind};
use application::tone3000::models_pick::unique_models;
use application::tone3000::name_tokens::{remove_constant_tokens, tokenize};
use plugin_loader::manifest::{GridParameter, ParameterValue};

fn models(fixture: &str) -> Vec<Model> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/tone3000")
        .join(fixture);
    let page: Page<Model> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    page.data
}

fn names_of(models: &[Model]) -> Vec<String> {
    models.iter().map(|m| m.name.clone()).collect()
}

fn axes_for(fixture: &str, kind: CaptureKind) -> (Vec<String>, Axes) {
    let names = names_of(&unique_models(models(fixture)));
    let axes = infer_axes(&names, kind);
    (names, axes)
}

fn strings(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

fn num(v: f64) -> ParameterValue {
    ParameterValue::Number(v)
}

fn text(v: &str) -> ParameterValue {
    ParameterValue::Text(v.to_string())
}

fn axis<'a>(axes: &'a Axes, name: &str) -> &'a GridParameter {
    axes.parameters
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no axis `{name}` in {:?}", axes.parameters))
}

fn axis_names(axes: &Axes) -> Vec<&str> {
    axes.parameters.iter().map(|p| p.name.as_str()).collect()
}

fn values_of(axes: &Axes, names: &[String], capture: &str) -> Vec<(String, ParameterValue)> {
    let index = names.iter().position(|n| n == capture).unwrap();
    axes.values[index]
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

#[test]
fn rows_that_share_a_file_collapse_to_one_capture() {
    let rows = models("models_66606_a2.json");
    assert_eq!(rows.len(), 20);
    let unique = unique_models(rows);
    assert_eq!(unique.len(), 10);
    let mut names = names_of(&unique);
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 10);
}

#[test]
fn a_row_without_a_file_is_skipped() {
    let mut rows = models("models_716_a1.json");
    rows[0].model_url = None;
    assert_eq!(unique_models(rows).len(), 4);
}

#[test]
fn tokens_are_lowercase_words_with_leading_zeros_stripped() {
    assert_eq!(
        tokenize("MRS - V30-01 - SM57 - 02 (eoc++)_dc"),
        strings(&["mrs", "v30", "1", "sm57", "2", "eoc", "dc"])
    );
    assert_eq!(tokenize("#0b JCM"), strings(&["0b", "jcm"]));
    assert_eq!(tokenize("Crunch 7.5"), strings(&["crunch", "7.5"]));
}

#[test]
fn a_knob_word_and_its_setting_become_one_token() {
    assert_eq!(tokenize("Gain 7.5 Bass 3"), strings(&["gain7.5", "bass3"]));
    assert_eq!(tokenize("SM57 Cap Edge"), strings(&["sm57", "cap_edge"]));
    assert_eq!(tokenize("C414 12 inch"), strings(&["c414", "12_inch"]));
}

#[test]
fn constant_tokens_leave_only_what_varies() {
    let names = vec![tokenize("A 01 SM57 01 c"), tokenize("A 01 SM57 02 eoc")];
    assert_eq!(
        remove_constant_tokens(&names),
        vec![strings(&["1", "c"]), strings(&["2", "eoc"])]
    );
}

#[test]
fn a_master_and_gain_grid_becomes_two_numeric_knobs() {
    let (names, axes) = axes_for("models_1071_a1.json", CaptureKind::Nam);
    assert_eq!(axis_names(&axes), ["master", "gain"]);
    assert_eq!(axis(&axes, "master").values, [num(5.0), num(6.0), num(7.0)]);
    assert_eq!(
        axis(&axes, "gain").values,
        (1..=10).map(|g| num(g as f64)).collect::<Vec<_>>()
    );
    assert_eq!(
        axis(&axes, "master").display_name.as_deref(),
        Some("Master")
    );
    assert_eq!(
        values_of(
            &axes,
            &names,
            "JCM800 2203 - P5 B5 M5 T5 MV6 G7 - AZG - 700"
        ),
        [
            ("gain".to_string(), num(7.0)),
            ("master".to_string(), num(6.0))
        ]
    );
}

#[test]
fn named_channels_become_one_preset_choice() {
    let (names, axes) = axes_for("models_66606_a2.json", CaptureKind::Nam);
    assert_eq!(axis_names(&axes), ["preset"]);
    assert_eq!(
        axis(&axes, "preset").values,
        [
            "clean_1",
            "clean_2",
            "crunch_1",
            "crunch_2",
            "crunch_3",
            "rhythm_1",
            "rhythm_2",
            "rhythm_3",
            "rhythm_4",
            "rhythm_5_plumes_od"
        ]
        .map(text)
    );
    assert_eq!(
        values_of(&axes, &names, "RHYTHM 5 PLUMES OD"),
        [("preset".to_string(), text("rhythm_5_plumes_od"))]
    );
}

#[test]
fn knobs_missing_from_some_captures_drop_out_of_the_preset_names() {
    let (names, axes) = axes_for("models_716_a1.json", CaptureKind::Nam);
    assert_eq!(axis_names(&axes), ["preset"]);
    assert_eq!(
        axis(&axes, "preset").values,
        ["0b", "0d_sd1", "1", "2", "3"].map(text)
    );
    assert_eq!(
        values_of(
            &axes,
            &names,
            "#0d 1989 Marshall JCM800 2203 + SD1 suhrRL-vossen"
        ),
        [("preset".to_string(), text("0d_sd1"))]
    );
}

#[test]
fn an_ir_pack_splits_into_mic_and_position_preset() {
    let (names, axes) = axes_for("models_67544_ir.json", CaptureKind::Ir);
    assert_eq!(axis_names(&axes), ["mic", "preset"]);
    assert_eq!(
        axis(&axes, "mic").values,
        ["md421", "r10", "sm57"].map(text)
    );
    assert_eq!(
        axis(&axes, "preset").values,
        ["1_c", "2_eoc", "3_eoc", "4_eoc", "5_eoc", "6_eoc"].map(text)
    );
    assert_eq!(
        values_of(&axes, &names, "MRS - V30-01 - MD421K - 03 (eoc+)_dc"),
        [
            ("mic".to_string(), text("md421")),
            ("preset".to_string(), text("3_eoc"))
        ]
    );
}

#[test]
fn an_ir_pack_without_known_words_names_every_capture() {
    let (_, axes) = axes_for("models_31292_ir.json", CaptureKind::Ir);
    assert_eq!(axis_names(&axes), ["preset"]);
    assert_eq!(
        axis(&axes, "preset").values,
        [
            "egnl01",
            "egnl02",
            "egnl03",
            "mesaosick_ii_1",
            "mesaosick_ii_2",
            "mesaosick_ii_3",
            "mesatrady412_1",
            "mesatrady412_2",
            "mesatrady412_3",
            "mrsh01_48",
            "mrsh02_48",
            "mrsh03_48",
            "va5153_1",
            "va5153_2",
            "va5153_3"
        ]
        .map(text)
    );
}

#[test]
fn a_single_capture_has_no_axes() {
    let axes = infer_axes(&strings(&["Only One"]), CaptureKind::Nam);
    assert!(axes.parameters.is_empty());
    assert_eq!(axes.values.len(), 1);
    assert!(axes.values[0].is_empty());
}

#[test]
fn numeric_preset_names_become_numbers() {
    let axes = infer_axes(&strings(&["Take 1", "Take 2", "Take 3"]), CaptureKind::Ir);
    assert_eq!(axis_names(&axes), ["preset"]);
    assert_eq!(axis(&axes, "preset").values, [num(1.0), num(2.0), num(3.0)]);
}

#[test]
fn identical_names_fall_back_to_numbered_presets() {
    let axes = infer_axes(&strings(&["Same", "Same"]), CaptureKind::Nam);
    assert_eq!(axis_names(&axes), ["preset"]);
    assert_eq!(axis(&axes, "preset").values, [num(1.0), num(2.0)]);
    assert_eq!(axes.values[0]["preset"], num(1.0));
    assert_eq!(axes.values[1]["preset"], num(2.0));
}

#[test]
fn a_knob_missing_from_a_capture_reads_minus_one() {
    let names = strings(&["Gain 5 Bass 3", "Gain 7", "Gain 5"]);
    let axes = infer_axes(&names, CaptureKind::Nam);
    assert_eq!(axis_names(&axes), ["gain", "bass"]);
    assert_eq!(axis(&axes, "gain").values, [num(5.0), num(7.0)]);
    assert_eq!(axis(&axes, "bass").values, [num(-1.0), num(3.0)]);
    assert_eq!(axes.values[1]["bass"], num(-1.0));
}

#[test]
fn mic_and_position_words_become_choices() {
    let names = strings(&["Amp SM57 Cap", "Amp SM57 Cone", "Amp MD421 Cap"]);
    let axes = infer_axes(&names, CaptureKind::Nam);
    assert_eq!(axis_names(&axes), ["mic", "position"]);
    assert_eq!(axis(&axes, "mic").values, ["md421", "sm57"].map(text));
    assert_eq!(axis(&axes, "position").values, ["cap", "cone"].map(text));
}

#[test]
fn a_two_word_position_survives_the_constant_words() {
    let axes = infer_axes(&strings(&["SM57 Cap Edge", "SM57 Cap"]), CaptureKind::Ir);
    assert_eq!(axis_names(&axes), ["position"]);
    assert_eq!(
        axis(&axes, "position").values,
        ["cap", "cap_edge"].map(text)
    );
}

#[test]
fn single_letter_knobs_need_company() {
    // A lone `v30` is a speaker, not volume 30: single letters other than
    // `g` only read as knobs next to another knob-shaped token.
    let axes = infer_axes(&strings(&["Cab V30", "Cab V12"]), CaptureKind::Nam);
    assert_eq!(axis_names(&axes), ["preset"]);
    assert_eq!(axis(&axes, "preset").values, ["v12", "v30"].map(text));
}

#[test]
fn ir_captures_never_read_knobs() {
    let axes = infer_axes(&strings(&["Gain 5", "Gain 7"]), CaptureKind::Ir);
    assert_eq!(axis_names(&axes), ["preset"]);
    assert_eq!(axis(&axes, "preset").values, ["gain5", "gain7"].map(text));
}

#[test]
fn a_dist_setting_is_the_distortion_knob() {
    let names: Vec<String> = [2, 4, 6, 8]
        .iter()
        .flat_map(|t| (0..=10).map(move |d| format!("DS-1(Mod)_ Tone{t}-Dist{d}")))
        .collect();
    let axes = infer_axes(&names, CaptureKind::Nam);
    assert_eq!(axis_names(&axes), ["tone", "distortion"]);
    assert_eq!(
        axis(&axes, "distortion").values,
        (0..=10).map(|d| num(d as f64)).collect::<Vec<_>>()
    );
    assert_eq!(
        axis(&axes, "distortion").display_name.as_deref(),
        Some("Distortion")
    );
}
