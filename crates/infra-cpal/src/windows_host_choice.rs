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

#[cfg(test)]
#[path = "windows_host_choice_tests.rs"]
mod tests;
