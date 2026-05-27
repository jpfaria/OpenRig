//! Generate per-stem waveform thumbnails (PNG) from the stereo WAV
//! the pipeline wrote out.
//!
//! The thumbnail is a downsampled RMS envelope of the mono mix of
//! the two channels: each output pixel column collapses a window of
//! input frames into a min / max line. PNG so the Slint side can
//! render it as a plain `Image` with no per-frame compute.

use std::path::Path;

use image::{ImageBuffer, Rgba, RgbaImage};

use crate::StemError;

/// Target peak image dimensions. ~1200 columns gives ~3 px per second
/// for a 5-minute track; 96 rows is enough resolution to see dynamics
/// without dominating the strip.
pub(crate) const PEAKS_WIDTH: u32 = 1200;
pub(crate) const PEAKS_HEIGHT: u32 = 96;

const COLOR_BG: Rgba<u8> = Rgba([0, 0, 0, 0]);
const COLOR_WAVE_FILL: Rgba<u8> = Rgba([74, 158, 255, 220]);
const COLOR_WAVE_BORDER: Rgba<u8> = Rgba([110, 200, 255, 255]);

/// Render a peak image for `samples` (interleaved stereo `f32`) at
/// the given dimensions and write it to `out_path` as PNG.
///
/// # Errors
///
/// - [`StemError::OpenSource`] when the image cannot be encoded or
///   written.
pub(crate) fn render_peaks_png(samples: &[f32], out_path: &Path) -> Result<(), StemError> {
    let frames = samples.len() / 2;
    let mut img: RgbaImage = ImageBuffer::from_pixel(PEAKS_WIDTH, PEAKS_HEIGHT, COLOR_BG);
    if frames == 0 {
        return write_png(&img, out_path);
    }

    let frames_per_col = (frames as f32 / PEAKS_WIDTH as f32).ceil().max(1.0) as usize;
    let mid_y = (PEAKS_HEIGHT / 2) as i32;
    let half = (PEAKS_HEIGHT / 2) as i32 - 1;

    for col in 0..PEAKS_WIDTH {
        let start = (col as usize) * frames_per_col;
        let end = ((col as usize + 1) * frames_per_col).min(frames);
        if start >= end {
            break;
        }
        // Mix down to mono, compute min/max.
        let mut min = 0.0_f32;
        let mut max = 0.0_f32;
        for frame in start..end {
            let l = samples[frame * 2];
            let r = samples[frame * 2 + 1];
            let m = 0.5 * (l + r);
            if m < min {
                min = m;
            }
            if m > max {
                max = m;
            }
        }
        let y_top = (mid_y as f32 - max.clamp(-1.0, 1.0) * half as f32) as i32;
        let y_bot = (mid_y as f32 - min.clamp(-1.0, 1.0) * half as f32) as i32;
        let y_top = y_top.clamp(0, PEAKS_HEIGHT as i32 - 1) as u32;
        let y_bot = y_bot.clamp(0, PEAKS_HEIGHT as i32 - 1) as u32;
        for y in y_top..=y_bot {
            img.put_pixel(col, y, COLOR_WAVE_FILL);
        }
        // Top/bottom border accent.
        img.put_pixel(col, y_top, COLOR_WAVE_BORDER);
        img.put_pixel(col, y_bot, COLOR_WAVE_BORDER);
    }

    write_png(&img, out_path)
}

fn write_png(img: &RgbaImage, out_path: &Path) -> Result<(), StemError> {
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| StemError::OpenSource {
            path: parent.to_path_buf(),
            source: err,
        })?;
    }
    img.save(out_path).map_err(|err| StemError::OpenSource {
        path: out_path.to_path_buf(),
        source: std::io::Error::other(err.to_string()),
    })
}
