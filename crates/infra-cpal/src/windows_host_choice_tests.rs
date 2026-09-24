use super::{choose_windows_host, WindowsHost};

#[test]
fn no_asio_driver_means_wasapi() {
    // cpal reports ASIO as available on every Windows machine, driver or not
    // (#978), so the device count is the only real signal. Without this, a PC
    // with onboard audio or a class-compliant interface had no devices at all.
    assert_eq!(choose_windows_host(0), WindowsHost::Wasapi);
}

#[test]
fn an_asio_driver_means_asio() {
    assert_eq!(choose_windows_host(1), WindowsHost::Asio);
    assert_eq!(choose_windows_host(3), WindowsHost::Asio);
}
