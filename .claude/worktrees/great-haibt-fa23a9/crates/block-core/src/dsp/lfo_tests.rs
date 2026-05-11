use super::*;

#[test]
fn sine_is_bounded() {
    let mut lfo = Lfo::new(LfoShape::Sine, 4.0, 44_100.0);
    for _ in 0..44_100 {
        let s = lfo.next();
        assert!(s.is_finite() && s.abs() <= 1.0 + 1e-6, "out of range: {s}");
    }
}

#[test]
fn saw_is_bounded() {
    let mut lfo = Lfo::new(LfoShape::Saw, 4.0, 44_100.0);
    for _ in 0..44_100 {
        let s = lfo.next();
        assert!(s.is_finite() && s.abs() <= 1.0 + 0.5, "out of range: {s}");
    }
}

#[test]
fn triangle_is_bounded() {
    let mut lfo = Lfo::new(LfoShape::Triangle, 4.0, 44_100.0);
    for _ in 0..44_100 {
        let s = lfo.next();
        assert!(s.is_finite() && s.abs() <= 1.0 + 0.1, "out of range: {s}");
    }
}

#[test]
fn unipolar_in_zero_one() {
    let mut lfo = Lfo::new(LfoShape::Sine, 4.0, 44_100.0);
    for _ in 0..44_100 {
        let s = lfo.next_unipolar();
        assert!(s.is_finite() && s >= -1e-6 && s <= 1.0 + 1e-6);
    }
}

#[test]
fn rate_change_takes_effect() {
    let mut lfo = Lfo::new(LfoShape::Sine, 1.0, 44_100.0);
    lfo.set_rate(10.0);
    assert!((lfo.rate_hz() - 10.0).abs() < 1e-9);
}
