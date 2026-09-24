//! Responsibility: converts f32 output samples to a device's integer sample format.

pub(crate) fn f32_to_i16(sample: f32) -> i16 {
    (sample * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

pub(crate) fn f32_to_u16(sample: f32) -> u16 {
    ((sample + 1.0) * 0.5 * u16::MAX as f32).clamp(0.0, u16::MAX as f32) as u16
}

pub(crate) fn f32_to_i32(sample: f32) -> i32 {
    (sample * i32::MAX as f32).clamp(i32::MIN as f32, i32::MAX as f32) as i32
}
