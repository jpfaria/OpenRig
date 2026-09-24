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

#[test]
fn the_host_is_decided_once_per_process() {
    use super::HostDecision;
    let decision = HostDecision::new();
    assert_eq!(decision.decide(|| 0), WindowsHost::Wasapi);
    // The interface came on after the first lookup: the picker and the
    // streams must still agree on the host chosen first.
    assert_eq!(decision.decide(|| 2), WindowsHost::Wasapi);
}

#[test]
fn a_decided_host_does_not_count_again() {
    use super::HostDecision;
    let decision = HostDecision::new();
    decision.decide(|| 1);
    decision.decide(|| panic!("the ASIO devices were counted a second time"));
}
