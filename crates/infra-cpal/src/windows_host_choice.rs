//! Responsibility: decides which Windows audio host the app opens.

/// The Windows audio hosts OpenRig can open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowsHost {
    Asio,
    Wasapi,
}

/// ASIO when it has a device to offer, WASAPI otherwise.
pub(crate) fn choose_windows_host(asio_devices: usize) -> WindowsHost {
    if asio_devices > 0 {
        WindowsHost::Asio
    } else {
        WindowsHost::Wasapi
    }
}

/// The host choice, made once per process. The device picker and the streams
/// look devices up through separate `cpal::Host` instances; if each counted
/// ASIO devices itself, an interface switched on (or a DAW releasing its ASIO
/// driver) between the two counts would put the picker on WASAPI and the
/// streams on ASIO, and every saved device id would miss (#978, #422).
pub(crate) struct HostDecision(std::sync::OnceLock<WindowsHost>);

impl HostDecision {
    pub(crate) const fn new() -> Self {
        Self(std::sync::OnceLock::new())
    }

    /// The decision, counting ASIO devices only the first time.
    pub(crate) fn decide(&self, count_asio_devices: impl FnOnce() -> usize) -> WindowsHost {
        *self
            .0
            .get_or_init(|| choose_windows_host(count_asio_devices()))
    }
}

#[cfg(test)]
#[path = "windows_host_choice_tests.rs"]
mod tests;
