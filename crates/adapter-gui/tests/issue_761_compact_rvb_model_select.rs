//! Issue #761: in the compact view, opening a block's plugin/model
//! selector and picking a different option does not apply — the block
//! keeps showing/using the previous plugin. Reproduces even with the
//! chain disabled, so it is not an audio-thread/runtime issue. The
//! block's other controls (enable footswitch, parameter knobs) on the
//! SAME row update normally — only the model swap is stuck (confirmed
//! with the reporting user), which rules out a systemic runtime-sync
//! problem and narrows the defect to the model-replace path itself.
//!
//! Root cause: `compact_chain_block_handlers.rs`'s `on_choose_block_model`
//! dispatches `Command::ReplaceBlockModel` and, on failure (e.g. the
//! target model isn't actually available/buildable), only
//! `log::error!`s and returns — it never calls `set_status_error` to
//! tell the user anything happened. Every OTHER error branch in the very
//! same closure (the `sync_live_chain_runtime` failure right below it)
//! DOES call `set_status_error`. So when the dispatch itself fails, the
//! compact view just sits there showing the old plugin with no
//! indication why — exactly the reported symptom.

use std::path::PathBuf;

fn compact_block_handlers_source() -> String {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/compact_chain_block_handlers.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The `ReplaceBlockModel` dispatch's error-handling body, bounded from
/// the dispatch call to the next sibling statement (the
/// `sync_live_chain_runtime` call) — so the assertion can't accidentally
/// pass by matching `set_status_error` calls used elsewhere in the same
/// closure (e.g. the `sync_live_chain_runtime` error branch right after).
fn replace_block_model_dispatch_error_body(src: &str) -> String {
    let needle = "session.dispatcher.dispatch(Command::ReplaceBlockModel";
    let start = src
        .find(needle)
        .unwrap_or_else(|| panic!("compact_chain_block_handlers.rs has no `{needle}` call"));
    let rest = &src[start..];
    let end = rest
        .find("sync_live_chain_runtime")
        .unwrap_or_else(|| panic!("expected `sync_live_chain_runtime` to follow the dispatch"));
    rest[..end].to_string()
}

#[test]
fn compact_choose_model_shows_a_toast_when_the_replace_dispatch_fails() {
    let src = compact_block_handlers_source();
    let body = replace_block_model_dispatch_error_body(&src);
    assert!(
        body.contains("set_status_error"),
        "in compact_chain_block_handlers.rs, the `Command::ReplaceBlockModel` \
         dispatch's error branch only logs (`log::error!`) and returns — it \
         never calls `set_status_error` to tell the user the model swap \
         failed. Every other error branch in the same closure does. Without \
         this, picking a plugin that fails to apply (#761) looks like \
         nothing happened at all: no toast, no visual change, no clue why. \
         Body read:\n{body}"
    );
}

fn compact_chain_callbacks_source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/compact_chain_callbacks.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The `on_choose_block_model_by_id` closure body, bounded from its
/// registration to the next sibling wiring block (the block-CRUD/param
/// handlers extracted-module calls right after it) — so the assertion
/// can't accidentally pass by matching `set_status_error` calls used
/// elsewhere in the file.
fn choose_block_model_by_id_body(src: &str) -> String {
    let needle = "on_choose_block_model_by_id";
    let start = src
        .find(needle)
        .unwrap_or_else(|| panic!("compact_chain_callbacks.rs has no `{needle}` handler"));
    let rest = &src[start..];
    let end = rest
        .find("compact_chain_block_handlers::wire")
        .unwrap_or_else(|| {
            panic!("expected `compact_chain_block_handlers::wire` to follow the handler")
        });
    rest[..end].to_string()
}

#[test]
fn compact_choose_model_by_id_logs_when_model_id_resolution_fails() {
    let src = compact_chain_callbacks_source();
    let body = choose_block_model_by_id_body(&src);
    assert!(
        body.contains("log::warn"),
        "in compact_chain_callbacks.rs, `on_choose_block_model_by_id` \
         silently returns with NO log at all when \
         `resolve_model_id_in_compact_block` can't find the clicked \
         model_id in the block's model list. The standalone block-editor \
         window's equivalent path \
         (`model_search_wiring::wire_standalone_block_editor_window`) logs \
         a warning on the same failure — the compact view logs nothing, \
         so a resolution failure is completely untraceable and looks \
         identical to the reported symptom (#761): the popup opens, the \
         click registers, and nothing happens, with zero trace of why. \
         Body read:\n{body}"
    );
}
