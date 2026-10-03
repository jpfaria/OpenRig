//! #324: the main window opens 1324 px wide by default, wide enough for the
//! chain row with the DI, looper and player controls.

use adapter_gui::AppWindow;
use slint::ComponentHandle;

#[test]
fn main_window_opens_at_the_default_width() {
    i_slint_backend_testing::init_no_event_loop();
    let w = AppWindow::new().unwrap();
    w.show().unwrap();

    assert_eq!(w.window().size().width, 1324);
}
