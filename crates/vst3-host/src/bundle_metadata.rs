//! Responsibility: reads what a VST3 bundle says about itself on disk.

use crate::discovery::Vst3PluginInfo;
use std::path::Path;

/// Parse `Contents/Resources/moduleinfo.json` without loading the plugin dylib.
///
/// Returns `None` if the file doesn't exist or can't be parsed.
pub(crate) fn read_moduleinfo(bundle_path: &Path) -> Option<Vec<Vst3PluginInfo>> {
    let json_path = bundle_path
        .join("Contents")
        .join("Resources")
        .join("moduleinfo.json");

    let raw = std::fs::read_to_string(&json_path).ok()?;

    // Parse vendor from Factory Info
    let vendor = extract_json_string(&raw, "Vendor").unwrap_or_default();

    // Find all "Audio Module Class" entries in the Classes array.
    let mut results = Vec::new();
    let mut pos = 0;
    while let Some(class_start) = raw[pos..].find("\"CID\"") {
        let base = pos + class_start;
        let chunk_end = raw[base..]
            .find('}')
            .map(|i| base + i + 1)
            .unwrap_or(raw.len());
        let chunk = &raw[base..chunk_end];

        let category = extract_json_string(chunk, "Category").unwrap_or_default();
        if !category.contains("Audio Module Class") {
            pos = chunk_end;
            continue;
        }

        let cid_hex = extract_json_string(chunk, "CID").unwrap_or_default();
        let uid = parse_cid_hex(&cid_hex);
        let name = extract_json_string(chunk, "Name").unwrap_or_else(|| "Unknown".to_string());

        if let Some(uid) = uid {
            results.push(Vst3PluginInfo {
                uid,
                name,
                vendor: vendor.clone(),
                category,
                bundle_path: bundle_path.to_path_buf(),
                params: Vec::new(),
                num_audio_inputs: 2,
                num_audio_outputs: 2,
            });
        }
        pos = chunk_end;
    }

    if results.is_empty() {
        None
    } else {
        Some(results)
    }
}

/// Extract a JSON string value for a given key (simple, no full parser needed).
fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    let start = json.find(&needle)?;
    let after_key = &json[start + needle.len()..];
    let colon = after_key.find(':')? + 1;
    let after_colon = after_key[colon..].trim_start();
    if !after_colon.starts_with('"') {
        return None;
    }
    let inner = &after_colon[1..];
    let end = inner.find('"')?;
    Some(inner[..end].to_string())
}

/// Parse a 32-hex-char CID string (e.g. "ABCDEF019182FAEB476E617547637332") into
/// a 16-byte array.
fn parse_cid_hex(hex: &str) -> Option<[u8; 16]> {
    let hex = hex.trim();
    if hex.len() != 32 {
        return None;
    }
    let mut uid = [0u8; 16];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(chunk).ok()?;
        uid[i] = u8::from_str_radix(s, 16).ok()?;
    }
    Some(uid)
}

/// Read vendor from `Contents/Info.plist` (fallback for bundles without moduleinfo.json).
pub(crate) fn read_info_plist_vendor(bundle_path: &Path) -> String {
    let plist_path = bundle_path.join("Contents").join("Info.plist");
    let raw = match std::fs::read_to_string(&plist_path) {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    // Look for CFBundleName or NSHumanReadableCopyright as vendor hint.
    extract_plist_string(&raw, "CFBundleName").unwrap_or_default()
}

/// Extract a string value from an Apple plist (XML format) without a full parser.
fn extract_plist_string(plist: &str, key: &str) -> Option<String> {
    let needle = format!("<key>{}</key>", key);
    let start = plist.find(&needle)? + needle.len();
    let after = &plist[start..];
    let str_start = after.find("<string>")? + "<string>".len();
    let str_end = after[str_start..].find("</string>")?;
    Some(after[str_start..str_start + str_end].to_string())
}
