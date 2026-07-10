//! #251: re-opening a VST3 editor must NOT create a second plugin instance.
//!
//! Once a plugin's editor view has been attached to a window and that window is
//! closed, some plugins (ValhallaSupermassive) leave their module in a state
//! where the next `IPluginFactory::createInstance` fails with `result=-1` for
//! the rest of the process. Releasing the old instance and reloading does NOT
//! recover it. The only safe behaviour is to reuse the editor already open for
//! that model instead of building a new one.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::Result;
use project::vst3_editor::{PluginEditorHandle, Vst3EditorRegistry};

struct FakeHandle;
impl PluginEditorHandle for FakeHandle {}

#[test]
fn reopening_same_model_reuses_the_open_editor() {
    let opens = Arc::new(AtomicUsize::new(0));
    let mut reg = Vst3EditorRegistry::new();

    let opener = |opens: Arc<AtomicUsize>| {
        move || -> Result<Box<dyn PluginEditorHandle>> {
            opens.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(FakeHandle) as Box<dyn PluginEditorHandle>)
        }
    };

    reg.open_or_focus("valhalla", opener(opens.clone()))
        .expect("first open");
    reg.open_or_focus("valhalla", opener(opens.clone()))
        .expect("second open");

    assert_eq!(
        opens.load(Ordering::SeqCst),
        1,
        "re-opening the same model must reuse the open editor, not create a new instance"
    );
}

/// A handle that records when it is dropped (i.e. its window is closed).
struct DropCountingHandle(Arc<AtomicUsize>);
impl PluginEditorHandle for DropCountingHandle {}
impl Drop for DropCountingHandle {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn close_drops_the_window_so_reopen_rebuilds() {
    // #780: after the engine rebuilds a block's instance, the GUI closes its
    // stale editor. Closing drops the handle (its window), and the next open
    // must build a fresh editor on the new instance instead of re-focusing.
    let opens = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut reg = Vst3EditorRegistry::new();

    let drops_for_open = drops.clone();
    let opens_for_open = opens.clone();
    let opener = move || -> Result<Box<dyn PluginEditorHandle>> {
        opens_for_open.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(DropCountingHandle(drops_for_open.clone())) as Box<dyn PluginEditorHandle>)
    };

    reg.open_or_focus("rig:g:block:0", &opener).expect("open");
    reg.close("rig:g:block:0");
    assert_eq!(
        drops.load(Ordering::SeqCst),
        1,
        "close must drop the handle (closing the stale window)"
    );

    reg.open_or_focus("rig:g:block:0", &opener).expect("reopen");
    assert_eq!(
        opens.load(Ordering::SeqCst),
        2,
        "after close, re-open must build a fresh editor on the new instance"
    );
}
