use super::{add_draft, apply_learned_source, finalize_draft, format_trigger, Draft};
use project::midi::Source;

#[test]
fn format_trigger_program_change() {
    let s = Source::ProgramChange { program: 7 };
    assert_eq!(format_trigger(&s), "PC #7");
}

#[test]
fn format_trigger_note_on() {
    let s = Source::NoteOn {
        channel: 1,
        note: 60,
    };
    assert_eq!(format_trigger(&s), "Note On ch=1 #60");
}

#[test]
fn format_trigger_note_off() {
    let s = Source::NoteOff {
        channel: 2,
        note: 48,
    };
    assert_eq!(format_trigger(&s), "Note Off ch=2 #48");
}

#[test]
fn format_trigger_cc() {
    let s = Source::Cc {
        channel: 1,
        controller: 7,
    };
    assert_eq!(format_trigger(&s), "CC ch=1 #7");
}

#[test]
fn add_draft_appends_empty_row_in_learning_state() {
    let mut bindings = vec![];
    let mut drafts: Vec<Draft> = vec![];
    add_draft(&mut bindings, &mut drafts);
    assert_eq!(drafts.len(), 1);
    assert!(drafts[0].source.is_none());
    assert!(drafts[0].learning);
}

#[test]
fn apply_learned_source_fills_active_draft_only() {
    let mut drafts: Vec<Draft> = vec![Default::default(), Default::default()];
    drafts[1].learning = true;
    apply_learned_source(
        &mut drafts,
        Source::Cc {
            channel: 1,
            controller: 7,
        },
    );
    assert!(drafts[0].source.is_none());
    assert!(matches!(drafts[1].source, Some(Source::Cc { .. })));
    assert!(!drafts[1].learning, "learn auto-stops after capture");
}

#[test]
fn finalize_draft_merges_into_bindings_when_command_chosen() {
    let mut bindings = vec![];
    let draft = Draft {
        source: Some(Source::ProgramChange { program: 5 }),
        command: Some("SaveProject".into()),
        learning: false,
    };
    let ok = finalize_draft(&mut bindings, draft);
    assert!(ok);
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].command, "SaveProject");
}

#[test]
fn finalize_draft_skipped_when_missing_source_or_command() {
    let mut bindings = vec![];
    let only_source = Draft {
        source: Some(Source::ProgramChange { program: 5 }),
        command: None,
        learning: false,
    };
    assert!(!finalize_draft(&mut bindings, only_source));
    let only_cmd = Draft {
        source: None,
        command: Some("SaveProject".into()),
        learning: false,
    };
    assert!(!finalize_draft(&mut bindings, only_cmd));
    assert!(bindings.is_empty());
}
