use super::class_uid_matches;

// DECLARE_UID(0x12345678, 0x9ABCDEF0, 0x0F1E2D3C, 0x4B5A6978).
//
// In memory, a COM-compatible SDK build (Windows) lays Data1/Data2/Data3 out
// little-endian, which is what getClassInfo hands back there. Everywhere else
// the bytes are big-endian, in the order the four integers are written.
const WINDOWS_TUID: [u8; 16] = [
    0x78, 0x56, 0x34, 0x12, 0xBC, 0x9A, 0xF0, 0xDE, 0x0F, 0x1E, 0x2D, 0x3C, 0x4B, 0x5A, 0x69, 0x78,
];
const PLAIN_TUID: [u8; 16] = [
    0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x0F, 0x1E, 0x2D, 0x3C, 0x4B, 0x5A, 0x69, 0x78,
];

#[test]
fn class_uid_matches_identical_bytes() {
    assert!(class_uid_matches(&PLAIN_TUID, &PLAIN_TUID));
    assert!(class_uid_matches(&WINDOWS_TUID, &WINDOWS_TUID));
}

#[test]
fn class_uid_matches_windows_factory_against_moduleinfo_cid() {
    // moduleinfo.json written on Windows prints the CID in COM format
    // ("123456789ABCDEF00F1E2D3C4B5A6978"), which parses byte by byte into
    // PLAIN_TUID, while the factory there reports WINDOWS_TUID.
    assert!(class_uid_matches(&WINDOWS_TUID, &PLAIN_TUID));
}

#[test]
fn class_uid_matches_plain_factory_against_com_ordered_uid() {
    // The reverse pairing: a bundle whose moduleinfo carries the in-memory
    // Windows bytes, loaded by a host whose factory reports the plain layout.
    assert!(class_uid_matches(&PLAIN_TUID, &WINDOWS_TUID));
}

#[test]
fn class_uid_rejects_a_different_class() {
    let mut other = PLAIN_TUID;
    other[15] ^= 0xFF;
    assert!(!class_uid_matches(&PLAIN_TUID, &other));
    assert!(!class_uid_matches(&WINDOWS_TUID, &other));
}

#[test]
fn class_uid_rejects_bytes_swapped_past_data3() {
    // Only Data1..Data3 (bytes 0-7) change order between the layouts;
    // Data4 (bytes 8-15) is a plain byte array in both.
    let mut tail_swapped = PLAIN_TUID;
    tail_swapped.swap(8, 9);
    assert!(!class_uid_matches(&PLAIN_TUID, &tail_swapped));
}
