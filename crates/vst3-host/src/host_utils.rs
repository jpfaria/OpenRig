//! Internal helpers for the VST3 host. Lifted out of `host.rs` so the
//! main module stays under the size cap.
//!
//! All items are `pub(crate)` — only host code uses them.

use anyhow::{bail, Context, Result};
use std::ffi::c_char;
use std::path::Path;

use vst3::Steinberg::TUID;

/// Read a nul-terminated `c_char` array into a `String`, stopping at the
/// first NUL byte or the end of the buffer.
pub(crate) fn cstr_array_to_string(buf: &[c_char]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Read a UTF-16 `char16` (`u16`) array into a `String`.
pub(crate) fn char16_array_to_string(buf: &[u16]) -> String {
    let utf16: Vec<u16> = buf.iter().take_while(|&&c| c != 0).copied().collect();
    String::from_utf16_lossy(&utf16)
}

/// Convert a 16-byte TUID (signed `c_char`) to `[u8; 16]`.
pub(crate) fn tuid_to_bytes(tuid: &TUID) -> [u8; 16] {
    let mut out = [0u8; 16];
    for (i, &b) in tuid.iter().enumerate() {
        out[i] = b as u8;
    }
    out
}

/// Resolve the binary path inside a `.vst3` bundle directory.
///
/// Convention:
/// - macOS:   `Plugin.vst3/Contents/MacOS/Plugin`
/// - Windows: `Plugin.vst3/Contents/x86_64-win/Plugin.vst3`
/// - Linux:   `Plugin.vst3/Contents/x86_64-linux/Plugin.so`
pub fn bundle_binary_path(bundle_path: &Path) -> Result<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let contents = bundle_path.join("Contents").join("MacOS");
        let stem = bundle_path
            .file_stem()
            .context("bundle has no filename")?
            .to_string_lossy();
        let candidate = contents.join(stem.as_ref());
        if candidate.exists() {
            return Ok(candidate);
        }
        if contents.exists() {
            for entry in std::fs::read_dir(&contents)? {
                let path = entry?.path();
                if path.is_file() {
                    return Ok(path);
                }
            }
        }
        bail!("no binary found in {}", contents.display());
    }
    #[cfg(target_os = "windows")]
    {
        let contents = bundle_path.join("Contents").join("x86_64-win");
        let stem = bundle_path
            .file_stem()
            .context("bundle has no filename")?
            .to_string_lossy();
        let candidate = contents.join(format!("{}.vst3", stem));
        if candidate.exists() {
            return Ok(candidate);
        }
        bail!("no binary found in {}", contents.display());
    }
    #[cfg(target_os = "linux")]
    {
        let arch = if cfg!(target_arch = "x86_64") {
            "x86_64-linux"
        } else if cfg!(target_arch = "aarch64") {
            "aarch64-linux"
        } else {
            "x86_64-linux"
        };
        let contents = bundle_path.join("Contents").join(arch);
        let stem = bundle_path
            .file_stem()
            .context("bundle has no filename")?
            .to_string_lossy();
        let candidate = contents.join(format!("{}.so", stem));
        if candidate.exists() {
            return Ok(candidate);
        }
        if contents.exists() {
            for entry in std::fs::read_dir(&contents)? {
                let path = entry?.path();
                if path.extension().and_then(|e| e.to_str()) == Some("so") {
                    return Ok(path);
                }
            }
        }
        bail!("no binary found in {}", contents.display());
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    bail!("unsupported platform for VST3 bundle resolution");
}
