//! Responsibility: addresses one path of one of a chain's splits.
//!
//! #328 (spec §11.1): the commands that add, insert or move a block take an
//! optional `PathRef`; `None` means the chain's top-level block list. The
//! split is named by its block id and the path by its 0-based index, so the
//! address stays unambiguous at any depth.

use domain::ids::BlockId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PathRef {
    pub split: BlockId,
    /// 0-based; the GUI shows it as a letter (see [`path_letter`]).
    pub path: usize,
}

/// The letter the GUI shows for path `index`: A … Z, then AA, AB, ….
pub fn path_letter(index: usize) -> String {
    let mut n = index + 1;
    let mut out = Vec::new();
    while n > 0 {
        let rem = (n - 1) % 26;
        out.push(b'A' + rem as u8);
        n = (n - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).expect("ascii letters")
}
