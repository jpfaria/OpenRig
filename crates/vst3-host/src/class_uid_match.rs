//! Responsibility: decides whether a factory class id is the class a stored uid names.

/// `true` when `factory_cid` (as `getClassInfo` reports it) names the same
/// class as `wanted`.
///
/// A class id has two byte layouts. A COM-compatible SDK build (Windows) keeps
/// Data1/Data2/Data3 (bytes 0-7) little-endian in memory, and there the SDK
/// also prints `moduleinfo.json` CIDs in COM format, which parses back
/// byte-for-byte into the other layout. A bundle's `moduleinfo.json` may also
/// have been generated on a different OS than the one loading it. So a stored
/// uid matches in either layout (#978). Only the first 8 bytes differ, and a
/// different class in the swapped layout would need the same 16 bytes
/// rearranged: not a collision that happens by accident.
pub(crate) fn class_uid_matches(factory_cid: &[u8; 16], wanted: &[u8; 16]) -> bool {
    factory_cid == wanted || *factory_cid == com_layout_swapped(wanted)
}

/// The same class id in the other layout: Data1 (4 bytes), Data2 and Data3
/// (2 bytes each) reversed, Data4 untouched.
fn com_layout_swapped(uid: &[u8; 16]) -> [u8; 16] {
    let mut out = *uid;
    out[0..4].reverse();
    out[4..6].reverse();
    out[6..8].reverse();
    out
}

#[cfg(test)]
#[path = "class_uid_match_tests.rs"]
mod tests;
