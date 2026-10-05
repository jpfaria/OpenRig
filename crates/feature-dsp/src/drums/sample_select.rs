//! Responsibility: picks which sample of a piece a hit plays.

use super::kit::DrumLayer;

/// `(layer, sample)` for a hit at `velocity`; `round_robin` is the piece's
/// hit counter, so alternate samples of a layer take turns.
pub(crate) fn pick_sample(
    layers: &[DrumLayer],
    velocity: f32,
    round_robin: u32,
) -> Option<(usize, usize)> {
    let layer = layer_for_velocity(layers, velocity)?;
    let count = layers[layer].samples.len();
    if count == 0 {
        return None;
    }
    Some((layer, round_robin as usize % count))
}

/// The layer whose range holds `velocity`: ranges are half-open so a shared
/// boundary goes to the louder layer, and the top layer includes its max.
/// Outside every range, the nearest layer plays.
fn layer_for_velocity(layers: &[DrumLayer], velocity: f32) -> Option<usize> {
    if let Some(i) = layers
        .iter()
        .position(|l| velocity >= l.min_velocity && velocity < l.max_velocity)
    {
        return Some(i);
    }
    layers
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let distance = if velocity < l.min_velocity {
                l.min_velocity - velocity
            } else if velocity > l.max_velocity {
                velocity - l.max_velocity
            } else {
                0.0
            };
            (i, distance)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}
