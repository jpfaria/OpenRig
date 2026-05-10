//! Reads the `metadata.loudness` field that NAM trainers (v0.5+) bake
//! into the JSON header of every `.nam` capture. It's a real loudness
//! measurement done with guitar training data — far better than any
//! pink-noise probe at predicting how the model will sound in use.
//!
//! Used by `NamProcessor::new` to compensate every NAM toward a single
//! loudness target. Falls back to the runtime probe when the field is
//! absent (older .nam versions).

use std::fs::File;
use std::io::Read;

/// Maximum bytes scanned at the start of the `.nam` file to locate the
/// metadata block. The JSON header for current trainers is well under
/// 2 KiB; 4 KiB gives margin for header drift without paying to read
/// the whole multi-hundred-KiB file.
const HEADER_SCAN_BYTES: usize = 4096;

/// Returns the baked output loudness in dBFS for a `.nam` capture, if
/// present. Returns None for older captures whose JSON metadata
/// doesn't include the field, or when the file can't be read / parsed.
pub fn read_loudness_dbfs(model_path: &str) -> Option<f32> {
    let mut file = File::open(model_path).ok()?;
    let mut buf = vec![0u8; HEADER_SCAN_BYTES];
    let n = file.read(&mut buf).ok()?;
    let head = std::str::from_utf8(&buf[..n]).ok()?;
    extract_loudness_from_header(head)
}

fn extract_loudness_from_header(header: &str) -> Option<f32> {
    let key = "\"loudness\":";
    let i = header.find(key)?;
    let rest = &header[i + key.len()..];
    let end = rest
        .find(|c: char| {
            c != '-' && c != '.' && !c.is_ascii_digit() && c != 'e' && c != 'E' && c != '+'
        })
        .unwrap_or(rest.len());
    rest[..end].parse::<f32>().ok()
}

#[cfg(test)]
#[path = "baked_loudness_tests.rs"]
mod tests;
