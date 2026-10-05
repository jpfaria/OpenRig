use super::*;

#[test]
fn heard_follows_the_consumed_counter() {
    let mut position = HeardPosition::default();
    position.reset(10.0, 0);
    position.pushed(256, 10.1);
    position.pushed(256, 10.2);
    assert_eq!(position.heard(0), 10.0);
    assert_eq!(position.heard(300), 10.1);
    assert_eq!(position.heard(512), 10.2);
    assert_eq!(position.queued(512), 0);
}

#[test]
fn reset_starts_from_the_consumed_total() {
    let mut position = HeardPosition::default();
    position.pushed(1000, 5.0);
    position.reset(2.0, 1000);
    assert_eq!(position.heard(1000), 2.0);
    position.pushed(100, 2.5);
    assert_eq!(position.queued(1000), 100);
    assert_eq!(position.heard(1100), 2.5);
}
