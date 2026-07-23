use super::*;

#[test]
fn unique_port_gets_instance_zero() {
    let out = assign_instances(vec!["Solo Pedal".to_string()]);
    assert_eq!(out[0].key.instance, 0);
}

#[test]
fn two_same_named_ports_get_instances_one_and_two_in_order() {
    let out = assign_instances(vec!["USB MIDI".into(), "USB MIDI".into()]);
    assert_eq!(out[0].key.instance, 1);
    assert_eq!(out[1].key.instance, 2);
}

#[test]
fn mixed_unique_and_duplicates() {
    let out = assign_instances(vec![
        "Solo".into(),
        "USB MIDI".into(),
        "USB MIDI".into(),
        "Solo2".into(),
    ]);
    assert_eq!(out[0].key.instance, 0);
    assert_eq!(out[1].key.instance, 1);
    assert_eq!(out[2].key.instance, 2);
    assert_eq!(out[3].key.instance, 0);
}

#[test]
fn raw_name_is_preserved_verbatim() {
    let out = assign_instances(vec!["FOO  ".to_string()]);
    assert_eq!(out[0].raw_name, "FOO  ");
}
