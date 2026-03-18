use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};

pub fn list_devices() -> Result<Vec<String>> {
    let host = cpal::default_host();
    let mut devices = Vec::new();

    for d in host.input_devices()? {
        devices.push(format!("input: {}", d.name().unwrap_or_else(|_| "<unknown>".into())));
    }
    for d in host.output_devices()? {
        devices.push(format!("output: {}", d.name().unwrap_or_else(|_| "<unknown>".into())));
    }

    Ok(devices)
}
