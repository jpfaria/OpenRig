//! Only a track the user put in their own folder can be deleted; the tracks
//! that ship with the app and any file elsewhere are refused and left alone.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use infra_filesystem::PlayerConfig;
use project::project::Project;

use crate::command::{Command, PlayerCommand};
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::player_event::PlayerEvent;
use crate::player_library::PlayerLibraryDirs;
use crate::player_state::PlayerControlState;

struct Library {
    dispatcher: LocalDispatcher,
    bundled: tempfile::TempDir,
    user: tempfile::TempDir,
}

fn library() -> Library {
    let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    })));
    let bundled = tempfile::tempdir().unwrap();
    let user = tempfile::tempdir().unwrap();
    dispatcher.attach_player_state(Rc::new(RefCell::new(PlayerControlState::restored(
        &PlayerConfig::default(),
        PlayerLibraryDirs {
            bundled: Some(bundled.path().into()),
            user: Some(user.path().into()),
        },
        None,
    ))));
    Library {
        dispatcher,
        bundled,
        user,
    }
}

fn file_in(dir: &tempfile::TempDir, rel: &str) -> PathBuf {
    let path = dir.path().join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"x").unwrap();
    path
}

fn run(dispatcher: &LocalDispatcher, cmd: PlayerCommand) -> anyhow::Result<Vec<Event>> {
    dispatcher.dispatch(Command::Player(cmd))
}

fn delete(dispatcher: &LocalDispatcher, path: &PathBuf) -> anyhow::Result<Vec<Event>> {
    run(
        dispatcher,
        PlayerCommand::DeletePlayerTrack { path: path.clone() },
    )
}

#[test]
fn a_track_in_the_users_folder_is_deleted() {
    let lib = library();
    let track = file_in(&lib.user, "rhythm/My Jam - E.m4a");

    let events = delete(&lib.dispatcher, &track).expect("delete");

    assert!(!track.exists());
    assert_eq!(
        events,
        vec![Event::Player(PlayerEvent::TrackDeleted { path: track })]
    );
}

#[test]
fn a_track_that_ships_with_the_app_is_refused() {
    let lib = library();
    let track = file_in(&lib.bundled, "solo/Slow Blues - A.m4a");

    assert!(delete(&lib.dispatcher, &track).is_err());
    assert!(track.exists());
}

#[test]
fn a_file_outside_the_users_folder_is_refused() {
    let lib = library();
    let elsewhere = tempfile::tempdir().unwrap();
    let file = file_in(&elsewhere, "keep.m4a");
    let sneaky = lib.user.path().join("..").join(
        file.strip_prefix(elsewhere.path().parent().unwrap())
            .unwrap(),
    );

    assert!(delete(&lib.dispatcher, &file).is_err());
    assert!(delete(&lib.dispatcher, &sneaky).is_err());
    assert!(file.exists());
}

#[test]
fn a_file_that_is_not_a_track_is_refused() {
    let lib = library();
    let notes = file_in(&lib.user, "notes.txt");

    assert!(delete(&lib.dispatcher, &notes).is_err());
    assert!(notes.exists());
}

#[test]
fn deleting_the_loaded_track_stops_and_unloads_it() {
    let lib = library();
    let track = file_in(&lib.user, "My Jam - E.m4a");
    run(
        &lib.dispatcher,
        PlayerCommand::LoadPlayerTrack {
            path: track.clone(),
        },
    )
    .unwrap();

    delete(&lib.dispatcher, &track).expect("delete");

    let snapshot = lib.dispatcher.player_snapshot();
    assert_eq!(snapshot.track, None);
    assert!(!snapshot.playing);
}

#[test]
fn deleting_another_track_keeps_the_loaded_one() {
    let lib = library();
    let loaded = file_in(&lib.user, "Keep - A.m4a");
    let other = file_in(&lib.user, "Drop - E.m4a");
    run(
        &lib.dispatcher,
        PlayerCommand::LoadPlayerTrack {
            path: loaded.clone(),
        },
    )
    .unwrap();

    delete(&lib.dispatcher, &other).expect("delete");

    assert_eq!(lib.dispatcher.player_snapshot().track, Some(loaded));
}
