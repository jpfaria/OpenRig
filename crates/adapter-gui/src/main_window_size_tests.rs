use super::initial_size;

#[test]
fn main_window_opens_1324_wide() {
    let slint::WindowSize::Logical(size) = initial_size() else {
        panic!("the main window size is logical");
    };
    assert_eq!(size.width, 1324.0);
}
