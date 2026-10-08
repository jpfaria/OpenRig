//! Responsibility: writes the mark a stepped-input restart leaves on disk.
//!
//! #979: the automatic restart of a chain whose input arrives stepped cuts the
//! sound and explains nothing, so every restart leaves a folder under
//! `<user data>/incidents/stepped-input/`:
//!
//! - `input-<k>.wav`: the last seconds of input stream `k`, every channel of
//!   the device, as the HAL delivered them;
//! - `cycles-<k>.csv`: host time and frame range of each callback;
//! - `device-<k>.json`: the device as the OS reported it at the trip;
//! - `device-after-<k>.json`: the same, read again once the new streams run;
//! - `openrig.json`: the streams OpenRig had open and the chain's routes;
//! - `system-audio-log.txt`: the system's audio log of the minutes before
//!   (macOS, #1081, `stepped_input_system_log`).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use infra_cpal::{DeviceProbe, SteppedInputEvidence, StreamEvidence};

/// Marks kept on disk; older ones are removed.
const KEEP_MARKS: usize = 10;
/// How long after the restart the device is read again.
const AFTER_RESTART: Duration = Duration::from_secs(3);

/// Leave the mark of a chain about to be restarted. The device is read now,
/// before the restart changes it; the files are written off this thread.
pub(crate) fn leave_mark(evidence: SteppedInputEvidence) {
    let at_trip: Vec<DeviceProbe> = device_ids(&evidence)
        .map(infra_cpal::probe_input_device)
        .collect();
    let root = infra_filesystem::user_data_root()
        .join("incidents")
        .join("stepped-input");
    let spawned = std::thread::Builder::new()
        .name("stepped-input-mark".into())
        .spawn(move || {
            let probed = |device_id: &str| {
                at_trip
                    .iter()
                    .find(|p| p.device_id == device_id)
                    .cloned()
                    .unwrap_or_default()
            };
            let dir = match write_mark(&root, &evidence, &probed) {
                Ok(dir) => dir,
                Err(e) => {
                    log::error!("[{}] stepped input: no mark: {e}", evidence.chain_id);
                    return;
                }
            };
            match crate::stepped_input_system_log::write_system_log(
                &dir,
                &crate::stepped_input_system_log::read_audio_log,
            ) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::Unsupported => {}
                Err(e) => log::error!("[{}] stepped input: no system log: {e}", evidence.chain_id),
            }
            std::thread::sleep(AFTER_RESTART);
            if let Err(e) = write_after_probe(&dir, &evidence, &infra_cpal::probe_input_device) {
                log::error!("[{}] stepped input: no after-probe: {e}", evidence.chain_id);
            }
            if let Err(e) = prune_marks(&root, KEEP_MARKS) {
                log::error!("stepped input: old marks not pruned: {e}");
            }
            log::warn!(
                "[{}] stepped input: mark left at {}",
                evidence.chain_id,
                dir.display()
            );
        });
    if let Err(e) = spawned {
        log::error!("stepped input: mark thread not started: {e}");
    }
}

fn device_ids(evidence: &SteppedInputEvidence) -> impl Iterator<Item = &str> {
    evidence
        .streams
        .iter()
        .filter_map(|s| s.identity.device_id.as_deref())
}

/// Write the mark folder; `probe` reads a device by its cpal id.
pub(crate) fn write_mark(
    root: &Path,
    evidence: &SteppedInputEvidence,
    probe: &dyn Fn(&str) -> DeviceProbe,
) -> io::Result<PathBuf> {
    let dir = root.join(format!(
        "{}-{}",
        utc_stamp(evidence.taken_at),
        sanitized(&evidence.chain_id)
    ));
    fs::create_dir_all(&dir)?;
    for (k, stream) in evidence.streams.iter().enumerate() {
        write_wav(&dir.join(format!("input-{k}.wav")), stream)?;
        write_cycles(&dir.join(format!("cycles-{k}.csv")), stream)?;
        if let Some(device_id) = &stream.identity.device_id {
            write_json(&dir.join(format!("device-{k}.json")), &probe(device_id))?;
        }
    }
    write_json(&dir.join("openrig.json"), &openrig_json(evidence))?;
    Ok(dir)
}

/// Read every device of the mark again, next to what it was at the trip.
pub(crate) fn write_after_probe(
    dir: &Path,
    evidence: &SteppedInputEvidence,
    probe: &dyn Fn(&str) -> DeviceProbe,
) -> io::Result<()> {
    for (k, stream) in evidence.streams.iter().enumerate() {
        if let Some(device_id) = &stream.identity.device_id {
            write_json(
                &dir.join(format!("device-after-{k}.json")),
                &probe(device_id),
            )?;
        }
    }
    Ok(())
}

/// Keep the `keep` newest marks. Mark names start with their UTC time, so
/// name order is time order.
pub(crate) fn prune_marks(root: &Path, keep: usize) -> io::Result<()> {
    let mut marks: Vec<PathBuf> = fs::read_dir(root)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|entry| entry.path())
        .collect();
    marks.sort();
    let excess = marks.len().saturating_sub(keep);
    for old in &marks[..excess] {
        fs::remove_dir_all(old)?;
    }
    Ok(())
}

fn write_wav(path: &Path, stream: &StreamEvidence) -> io::Result<()> {
    let spec = hound::WavSpec {
        channels: stream.identity.channels as u16,
        sample_rate: stream.identity.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut wav = hound::WavWriter::create(path, spec).map_err(io::Error::other)?;
    for &sample in &stream.snapshot.samples {
        wav.write_sample(sample).map_err(io::Error::other)?;
    }
    wav.finalize().map_err(io::Error::other)
}

fn write_cycles(path: &Path, stream: &StreamEvidence) -> io::Result<()> {
    let mut csv = io::BufWriter::new(fs::File::create(path)?);
    writeln!(csv, "host_ns,first_frame,frames")?;
    for cycle in &stream.snapshot.cycles {
        writeln!(
            csv,
            "{},{},{}",
            cycle.host_ns, cycle.first_frame, cycle.frames
        )?;
    }
    csv.flush()
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let text = serde_json::to_string_pretty(value).map_err(io::Error::other)?;
    fs::write(path, text)
}

fn openrig_json(evidence: &SteppedInputEvidence) -> serde_json::Value {
    let streams: Vec<serde_json::Value> = evidence
        .streams
        .iter()
        .map(|stream| {
            let id = &stream.identity;
            let snapshot = &stream.snapshot;
            serde_json::json!({
                "input_index": id.input_index,
                "device_id": id.device_id,
                "sample_rate": id.sample_rate,
                "buffer_frames": id.buffer_frames,
                "channels": id.channels,
                "opened_at": utc_stamp(id.opened_at),
                "first_frame": snapshot.first_frame,
                "frames": snapshot.samples.len() / id.channels.max(1),
                "cycles": snapshot.cycles.len(),
            })
        })
        .collect();
    serde_json::json!({
        "chain_id": evidence.chain_id,
        "taken_at": utc_stamp(evidence.taken_at),
        "input_stepped": evidence.input_stepped,
        "app_version": env!("CARGO_PKG_VERSION"),
        "streams": streams,
        "routes": evidence.routes,
    })
}

/// `YYYYMMDD-HHMMSS` in UTC.
fn utc_stamp(at: SystemTime) -> String {
    let secs = at
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (year, month, day) = civil_date((secs / 86_400) as i64);
    let of_day = secs % 86_400;
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        of_day / 3600,
        of_day / 60 % 60,
        of_day % 60
    )
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian.
fn civil_date(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// A chain id as a folder name.
fn sanitized(chain_id: &str) -> String {
    chain_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "stepped_input_mark_tests.rs"]
mod tests;
