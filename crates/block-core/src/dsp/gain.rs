//! Responsibility: converts a gain between decibels the linear scale.

pub fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

pub fn lin_to_db(lin: f32) -> f32 {
    if lin > 1e-10 {
        20.0 * lin.log10()
    } else {
        -200.0
    }
}
