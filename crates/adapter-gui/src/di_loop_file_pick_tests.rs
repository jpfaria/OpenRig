//! #913 — pointing a chain's DI loop at a chosen file.
//!
//! The index comes from the chain tile the user touched, so a stale one must
//! resolve to "nothing to do" rather than pointing some other chain at the
//! file. And with no project open the pick is simply dropped — the popup can
//! still be on screen while the project closes under it.

use super::apply_di_loop_file;
use crate::state::ProjectSession;
use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

fn chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

fn session(chains: Vec<Chain>) -> Rc<RefCell<Option<ProjectSession>>> {
    Rc::new(RefCell::new(Some(ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains,
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-913-di-pick-tests"),
    ))))
}

/// A real (tiny, silent) WAV on disk: the dispatcher refuses a source whose
/// file does not exist, so a made-up path would never reach the chain.
fn a_wav(dir: &tempfile::TempDir, name: &str) -> PathBuf {
    let path = dir.path().join(name);
    let data: Vec<u8> = {
        let frames: u32 = 8;
        let bytes = frames * 2; // mono, 16-bit
        let mut w = Vec::new();
        w.extend(b"RIFF");
        w.extend((36 + bytes).to_le_bytes());
        w.extend(b"WAVEfmt ");
        w.extend(16u32.to_le_bytes());
        w.extend(1u16.to_le_bytes()); // PCM
        w.extend(1u16.to_le_bytes()); // mono
        w.extend(48_000u32.to_le_bytes());
        w.extend(96_000u32.to_le_bytes()); // byte rate
        w.extend(2u16.to_le_bytes()); // block align
        w.extend(16u16.to_le_bytes()); // bits
        w.extend(b"data");
        w.extend(bytes.to_le_bytes());
        w.extend(std::iter::repeat_n(0u8, bytes as usize));
        w
    };
    std::fs::write(&path, data).expect("write wav");
    path
}

#[test]
fn the_pick_reaches_the_chain_the_row_belongs_to() {
    let dir = tempfile::tempdir().expect("tempdir");
    let session = session(vec![chain("chain:0"), chain("chain:1")]);

    assert_eq!(
        apply_di_loop_file(&session, 1, a_wav(&dir, "loop.wav")),
        Ok(true),
        "the second row resolves to chain:1 and the command is accepted"
    );
}

#[test]
fn a_row_index_with_no_chain_behind_it_does_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let session = session(vec![chain("chain:0")]);
    assert_eq!(
        apply_di_loop_file(&session, 9, a_wav(&dir, "loop.wav")),
        Ok(false)
    );
}

#[test]
fn a_pick_with_no_project_open_is_dropped() {
    let dir = tempfile::tempdir().expect("tempdir");
    let none: Rc<RefCell<Option<ProjectSession>>> = Rc::new(RefCell::new(None));
    assert_eq!(
        apply_di_loop_file(&none, 0, a_wav(&dir, "loop.wav")),
        Ok(false)
    );
}

#[test]
fn picking_again_replaces_the_previous_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let session = session(vec![chain("chain:0")]);
    assert_eq!(
        apply_di_loop_file(&session, 0, a_wav(&dir, "first.wav")),
        Ok(true)
    );
    assert_eq!(
        apply_di_loop_file(&session, 0, a_wav(&dir, "second.wav")),
        Ok(true),
        "a second pick on the same chain is accepted, not rejected as a duplicate"
    );
}

#[test]
fn a_file_that_is_no_longer_there_is_reported_instead_of_silently_selected() {
    let session = session(vec![chain("chain:0")]);
    let gone = PathBuf::from("/nonexistent/openrig-913/gone.wav");
    let err = apply_di_loop_file(&session, 0, gone).expect_err("a missing file must be refused");
    assert!(err.contains("not found"), "unexpected message: {err}");
}
