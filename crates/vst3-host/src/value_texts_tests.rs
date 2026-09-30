use super::{continuous_value_texts, VALUE_TEXT_POSITIONS};

#[test]
fn one_text_per_knob_position_read_at_its_normalized_value() {
    let texts = continuous_value_texts("", |n| Some(format!("{:.2}", n)));
    assert_eq!(texts.len(), VALUE_TEXT_POSITIONS);
    assert_eq!(texts[0], "0.00");
    assert_eq!(texts[50], "0.50");
    assert_eq!(texts[100], "1.00");
}

#[test]
fn units_are_appended_when_the_text_lacks_them() {
    let texts = continuous_value_texts("s", |_| Some("2.5".to_string()));
    assert_eq!(texts[0], "2.5 s");
    let texts = continuous_value_texts("ms", |_| Some("120 ms".to_string()));
    assert_eq!(texts[0], "120 ms");
}

#[test]
fn no_texts_when_the_plugin_formats_nothing() {
    // Plugin without getParamStringByValue support: the knob keeps its number.
    assert!(continuous_value_texts("", |_| None).is_empty());
    assert!(continuous_value_texts("", |_| Some(String::new())).is_empty());
}
