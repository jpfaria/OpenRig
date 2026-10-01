//! Responsibility: reads one CoreAudio device's state into a `DeviceProbe`.
//!
//! What a stepped input needs ruled in or out: the rate the device runs at
//! against the rate the streams were opened at, the formats the HAL hands out
//! against the hardware's, the clock, the buffer, and which other processes
//! have the device open.

use crate::coreaudio_properties::{
    all_devices, bytes, device_for_cpal_id, f32_of, f64_of, fourcc, fourcc_text, i32_of,
    objects_of, string_of, u32_of, DEVICE_UID, SCOPE_GLOBAL, SCOPE_INPUT, SCOPE_OUTPUT,
    SYSTEM_OBJECT,
};
use crate::device_probe::{DeviceClient, DeviceProbe, ProbedStream, StreamFormat};

const NAME: u32 = fourcc(b"lnam");
const TRANSPORT: u32 = fourcc(b"tran");
const NOMINAL_RATE: u32 = fourcc(b"nsrt");
const ACTUAL_RATE: u32 = fourcc(b"asrt");
const BUFFER_FRAMES: u32 = fourcc(b"fsiz");
const RUNNING_SOMEWHERE: u32 = fourcc(b"gone");
const RUNNING: u32 = fourcc(b"goin");
const HOG_MODE: u32 = fourcc(b"oink");
const CLOCK_DOMAIN: u32 = fourcc(b"clkd");
const CLOCK_SOURCE: u32 = fourcc(b"csrc");
const CLOCK_DEVICE: u32 = fourcc(b"apcd");
const LATENCY: u32 = fourcc(b"ltnc");
const SAFETY_OFFSET: u32 = fourcc(b"saft");
const IO_CYCLE_USAGE: u32 = fourcc(b"ncyc");
const VARIABLE_BUFFER: u32 = fourcc(b"vfsz");
const STREAMS: u32 = fourcc(b"stm#");
const STARTING_CHANNEL: u32 = fourcc(b"schn");
const VIRTUAL_FORMAT: u32 = fourcc(b"sfmt");
const PHYSICAL_FORMAT: u32 = fourcc(b"pft ");
const PROCESSES: u32 = fourcc(b"prs#");
const PROCESS_PID: u32 = fourcc(b"ppid");
const PROCESS_BUNDLE_ID: u32 = fourcc(b"pbid");
const PROCESS_DEVICES: u32 = fourcc(b"pdv#");
const PROCESS_RUNNING_INPUT: u32 = fourcc(b"piri");
const PROCESS_RUNNING_OUTPUT: u32 = fourcc(b"piro");
const AGGREGATE_TRANSPORT: u32 = fourcc(b"grup");
const AGGREGATE_ACTIVE_SUBDEVICES: u32 = fourcc(b"agrp");

/// Read the device behind a cpal `coreaudio:<uid>` id.
pub(crate) fn probe(device_id: &str) -> DeviceProbe {
    let Some(device) = device_for_cpal_id(device_id) else {
        return DeviceProbe {
            device_id: device_id.to_string(),
            ..Default::default()
        };
    };
    let flag = |selector| u32_of(device, selector, SCOPE_GLOBAL).map(|v| v != 0);
    DeviceProbe {
        device_id: device_id.to_string(),
        found: true,
        name: string_of(device, NAME, SCOPE_GLOBAL),
        transport: u32_of(device, TRANSPORT, SCOPE_GLOBAL).map(fourcc_text),
        nominal_rate: f64_of(device, NOMINAL_RATE, SCOPE_GLOBAL),
        actual_rate: f64_of(device, ACTUAL_RATE, SCOPE_GLOBAL),
        buffer_frames: u32_of(device, BUFFER_FRAMES, SCOPE_GLOBAL),
        running_somewhere: flag(RUNNING_SOMEWHERE),
        running_here: flag(RUNNING),
        hog_pid: i32_of(device, HOG_MODE, SCOPE_GLOBAL),
        clock_domain: u32_of(device, CLOCK_DOMAIN, SCOPE_GLOBAL),
        clock_source: u32_of(device, CLOCK_SOURCE, SCOPE_GLOBAL)
            .or_else(|| u32_of(device, CLOCK_SOURCE, SCOPE_INPUT)),
        clock_device: string_of(device, CLOCK_DEVICE, SCOPE_GLOBAL),
        input_latency: u32_of(device, LATENCY, SCOPE_INPUT),
        input_safety_offset: u32_of(device, SAFETY_OFFSET, SCOPE_INPUT),
        io_cycle_usage: f32_of(device, IO_CYCLE_USAGE, SCOPE_GLOBAL),
        variable_buffer_frames: u32_of(device, VARIABLE_BUFFER, SCOPE_GLOBAL),
        input_streams: objects_of(device, STREAMS, SCOPE_INPUT)
            .into_iter()
            .map(probe_stream)
            .collect(),
        clients: clients_of(device),
        aggregates: aggregates_with(device),
    }
}

fn probe_stream(stream: u32) -> ProbedStream {
    ProbedStream {
        starting_channel: u32_of(stream, STARTING_CHANNEL, SCOPE_GLOBAL),
        latency: u32_of(stream, LATENCY, SCOPE_GLOBAL),
        virtual_format: bytes(stream, VIRTUAL_FORMAT, SCOPE_GLOBAL).and_then(|b| format(&b)),
        physical_format: bytes(stream, PHYSICAL_FORMAT, SCOPE_GLOBAL).and_then(|b| format(&b)),
    }
}

/// An `AudioStreamBasicDescription`: f64 rate, then eight u32 fields.
fn format(asbd: &[u8]) -> Option<StreamFormat> {
    if asbd.len() < 40 {
        return None;
    }
    let field = |k: usize| {
        let at = 8 + 4 * k;
        u32::from_ne_bytes([asbd[at], asbd[at + 1], asbd[at + 2], asbd[at + 3]])
    };
    Some(StreamFormat {
        sample_rate: f64::from_ne_bytes(asbd[..8].try_into().ok()?),
        format_id: fourcc_text(field(0)),
        flags: field(1),
        bytes_per_frame: field(4),
        channels: field(5),
        bits: field(6),
    })
}

/// Every process with the device open (macOS 14+ lists processes).
fn clients_of(device: u32) -> Vec<DeviceClient> {
    objects_of(SYSTEM_OBJECT, PROCESSES, SCOPE_GLOBAL)
        .into_iter()
        .filter_map(|process| {
            let input = objects_of(process, PROCESS_DEVICES, SCOPE_INPUT).contains(&device);
            let output = objects_of(process, PROCESS_DEVICES, SCOPE_OUTPUT).contains(&device);
            (input || output).then(|| DeviceClient {
                pid: i32_of(process, PROCESS_PID, SCOPE_GLOBAL).unwrap_or(-1),
                bundle_id: string_of(process, PROCESS_BUNDLE_ID, SCOPE_GLOBAL),
                input,
                output,
                running_input: u32_of(process, PROCESS_RUNNING_INPUT, SCOPE_GLOBAL).map(|v| v != 0),
                running_output: u32_of(process, PROCESS_RUNNING_OUTPUT, SCOPE_GLOBAL)
                    .map(|v| v != 0),
            })
        })
        .collect()
}

/// UIDs of the aggregate devices currently built on top of `device`.
fn aggregates_with(device: u32) -> Vec<String> {
    all_devices()
        .into_iter()
        .filter(|&d| u32_of(d, TRANSPORT, SCOPE_GLOBAL) == Some(AGGREGATE_TRANSPORT))
        .filter(|&d| objects_of(d, AGGREGATE_ACTIVE_SUBDEVICES, SCOPE_GLOBAL).contains(&device))
        .filter_map(|d| string_of(d, DEVICE_UID, SCOPE_GLOBAL))
        .collect()
}
