//! Audio decode: WAV/MP3/FLAC/OGG/M4A → interleaved stereo `f32`.

use std::fs::File;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::{DecodedAudio, StemError};

/// Decode an audio file into interleaved stereo `f32` samples at the
/// source sample rate.
///
/// Mono sources are broadcast to both channels (OpenRig stereo
/// invariant). Sources with more than two channels are downmixed to
/// stereo by averaging extra channels into L (even index) and R
/// (odd index).
///
/// # Errors
///
/// - [`StemError::OpenSource`] when the file cannot be opened.
/// - [`StemError::UnsupportedFormat`] when the container or codec is
///   unsupported, or no default audio track is present.
/// - [`StemError::Decode`] when decoding fails mid-stream.
pub fn decode_audio(path: &Path) -> Result<DecodedAudio, StemError> {
    let file = File::open(path).map_err(|source| StemError::OpenSource {
        path: path.to_path_buf(),
        source,
    })?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|err| unsupported(path, err))?;
    let mut format = probed.format;

    let track = format
        .default_track()
        .ok_or_else(|| StemError::UnsupportedFormat {
            path: path.to_path_buf(),
            reason: "no default audio track".to_string(),
        })?;
    let track_id = track.id;
    let codec_params = track.codec_params.clone();
    let sample_rate = codec_params
        .sample_rate
        .ok_or_else(|| StemError::UnsupportedFormat {
            path: path.to_path_buf(),
            reason: "missing sample rate".to_string(),
        })?;
    let channels = codec_params
        .channels
        .ok_or_else(|| StemError::UnsupportedFormat {
            path: path.to_path_buf(),
            reason: "missing channel layout".to_string(),
        })?;
    let source_channels = channels.count() as u16;

    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .map_err(|err| unsupported(path, err))?;

    let mut samples: Vec<f32> = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(err))
                if err.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(err) => return Err(decode_err(path, err)),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = decoder
            .decode(&packet)
            .map_err(|err| decode_err(path, err))?;

        let spec = *decoded.spec();
        let capacity = decoded.capacity() as u64;
        if sample_buf.is_none() {
            sample_buf = Some(SampleBuffer::<f32>::new(capacity, spec));
        }
        let buf = sample_buf
            .as_mut()
            .expect("sample buffer initialised on first packet");
        buf.copy_interleaved_ref(decoded);
        downmix_to_stereo(buf.samples(), source_channels, &mut samples);
    }

    Ok(DecodedAudio {
        samples,
        sample_rate,
        source_channels,
    })
}

fn unsupported(path: &Path, err: SymphoniaError) -> StemError {
    StemError::UnsupportedFormat {
        path: path.to_path_buf(),
        reason: err.to_string(),
    }
}

fn decode_err(path: &Path, err: SymphoniaError) -> StemError {
    StemError::Decode {
        path: path.to_path_buf(),
        reason: err.to_string(),
    }
}

/// Push interleaved `source_channels`-channel `f32` samples as stereo
/// (interleaved L/R) into `out`.
fn downmix_to_stereo(interleaved: &[f32], source_channels: u16, out: &mut Vec<f32>) {
    let ch = source_channels as usize;
    if ch == 0 {
        return;
    }
    let frames = interleaved.len() / ch;
    out.reserve(frames * 2);

    match ch {
        1 => {
            for &sample in interleaved {
                out.push(sample);
                out.push(sample);
            }
        }
        2 => out.extend_from_slice(interleaved),
        n => {
            for frame in 0..frames {
                let base = frame * n;
                let mut l_acc = interleaved[base];
                let mut r_acc = interleaved[base + 1];
                let mut l_count = 1.0_f32;
                let mut r_count = 1.0_f32;
                for (idx, sample) in interleaved[base + 2..base + n].iter().enumerate() {
                    if idx % 2 == 0 {
                        l_acc += sample;
                        l_count += 1.0;
                    } else {
                        r_acc += sample;
                        r_count += 1.0;
                    }
                }
                out.push(l_acc / l_count);
                out.push(r_acc / r_count);
            }
        }
    }
}
