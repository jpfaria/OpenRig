//! #398: the strips draw a unity mark where the fader law puts 0 dB.

use slint::Global;

use super::set_mixer_rows;
use crate::mixer_fader_law::position_from_db;
use crate::{CompactChainViewWindow, MixerBridge};

#[test]
fn drawing_the_rows_places_the_unity_mark() {
    i_slint_backend_testing::init_no_event_loop();
    let w = CompactChainViewWindow::new().unwrap();
    let bridge = MixerBridge::get(&w);
    set_mixer_rows(&bridge, Vec::new(), Vec::new());
    assert_eq!(bridge.get_unity_position(), position_from_db(0.0));
}
