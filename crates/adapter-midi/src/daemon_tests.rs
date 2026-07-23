use super::*;

#[test]
fn select_ports_returns_all_matching_so_several_pedals_work() {
    let ports = vec![
        "IAC Driver Bus 1".to_string(),
        "M-VAVE Chocolate".to_string(),
        "Chocolate Plus".to_string(),
    ];
    assert_eq!(select_ports(&ports, Some("chocolate")), vec![1, 2]);
}

#[test]
fn select_ports_none_opens_every_port() {
    let ports = vec!["A".to_string(), "B".to_string(), "C".to_string()];
    assert_eq!(select_ports(&ports, None), vec![0, 1, 2]);
}

#[test]
fn select_ports_empty_when_no_match() {
    let ports = vec!["A".to_string()];
    assert!(select_ports(&ports, Some("nope")).is_empty());
}

#[test]
fn select_ports_empty_when_no_ports() {
    assert!(select_ports(&[], None).is_empty());
}
