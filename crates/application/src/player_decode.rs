//! Responsibility: decodes a backing-track file into raw samples.
//!
//! Runs on the player's worker thread, never on the audio thread. The engine
//! takes the result from here and makes it stereo.

use std::fs::File;
use std::path::Path;

use engine::player::pcm::DecodedAudio;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

/// Every sample of the file's default audio track, interleaved, in the file's
/// own channel layout and rate.
pub fn decode_player_file(path: &Path) -> Result<DecodedAudio, String> {
    let file = File::open(path).map_err(|e| format!("cannot open: {e}"))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| format!("not a supported audio file: {e}"))?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or("the file has no audio track")?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .ok_or("the audio track has no codec parameters")?;
    let mut sample_rate = params.sample_rate.unwrap_or(0);
    let mut channels = params.channels.as_ref().map_or(0, |c| c.count());
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| format!("unsupported codec: {e}"))?;

    let mut samples: Vec<f32> = Vec::new();
    let mut block: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(format!("cannot read: {e}")),
        };
        if packet.track_id != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buffer) => {
                sample_rate = buffer.spec().rate();
                channels = buffer.spec().channels().count();
                block.resize(buffer.samples_interleaved(), 0.0);
                buffer.copy_to_slice_interleaved(&mut block);
                samples.extend_from_slice(&block);
            }
            // A damaged packet is skipped, as every player does.
            Err(Error::DecodeError(_)) => {}
            Err(e) => return Err(format!("cannot decode: {e}")),
        }
    }
    if samples.is_empty() || channels == 0 || sample_rate == 0 {
        return Err("the file holds no audio".into());
    }
    Ok(DecodedAudio {
        samples,
        channels,
        sample_rate,
    })
}

#[cfg(test)]
#[path = "player_decode_tests.rs"]
mod tests;
