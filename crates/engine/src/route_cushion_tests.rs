use super::*;

#[test]
fn a_same_clock_route_rests_at_its_own_target() {
    let c = route_cushion(128, 44_100.0, 44_100.0, false, true);
    assert_eq!(c.target, 128);
    assert_eq!(
        c.prime, 0,
        "a route no convolver feeds starts empty, as before"
    );
    assert!(!c.servo_owned);
}

#[test]
fn a_convolver_fed_route_is_born_at_its_rest_never_above_it() {
    let c = route_cushion(128, 44_100.0, 44_100.0, true, true);
    assert_eq!(c.target, 128);
    assert_eq!(c.prime, 128);
}

#[test]
fn a_cross_rate_route_rests_deeper_and_belongs_to_its_servo() {
    let c = route_cushion(128, 48_000.0, 44_100.0, false, false);
    assert_eq!(c.target, 128 * CROSS_RATE_CUSHION);
    assert_eq!(c.prime, c.target - 128, "its extra depth is filled");
    assert!(c.servo_owned);
    let fed = route_cushion(128, 48_000.0, 44_100.0, true, false);
    assert_eq!(fed.prime, fed.target);
    assert_eq!(
        fed.target,
        IR_COLD_START_CUSHION_FRAMES * CROSS_RATE_CUSHION,
        "an IR tap on another clock rests at the #592 cushion, cross-rate deep"
    );
}

#[test]
fn the_ring_holds_twice_the_cushion() {
    assert_eq!(
        route_cushion(64, 44_100.0, 44_100.0, false, true).capacity,
        128
    );
    assert_eq!(
        route_cushion(128, 48_000.0, 44_100.0, false, false).capacity,
        768
    );
}

#[test]
fn an_ir_route_keeps_the_592_cushion_only_off_its_producer_clock() {
    assert_eq!(
        route_cushion(128, 48_000.0, 48_000.0, true, true).target,
        128
    );
    let other = route_cushion(128, 48_000.0, 48_000.0, true, false);
    assert_eq!(other.target, IR_COLD_START_CUSHION_FRAMES);
    assert_eq!(other.prime, IR_COLD_START_CUSHION_FRAMES);
    assert_eq!(
        route_cushion(128, 48_000.0, 48_000.0, false, false).target,
        128
    );
}
