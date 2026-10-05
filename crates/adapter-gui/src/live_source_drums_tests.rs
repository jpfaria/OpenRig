//! The drums' read seam answers safely with no runtime: the panel polls it on
//! every frame, including before a project is open.

use super::drums_live_source;
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn with_no_runtime_there_is_no_drum_position_to_report() {
    let runtime = Rc::new(RefCell::new(None));
    let source = drums_live_source(&runtime);
    assert!(source.drums().is_none());
}
