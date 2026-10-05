//! Issue #1070 — the audio setup reaches the on-disk session log, not only
//! the crash reporter. Own binary: the published context is process-global.

use adapter_gui::crash_context::publish_audio;
use serde_json::json;

#[test]
fn issue_1070_the_audio_context_is_logged_only_when_it_changes() {
    let ctx = json!({ "chains": [{ "id": "issue-1070-log" }] });
    assert!(
        publish_audio(ctx.clone()),
        "first publish must reach the log"
    );
    assert!(
        !publish_audio(ctx),
        "an unchanged setup must not flood the log"
    );
}
