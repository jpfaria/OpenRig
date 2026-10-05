use super::*;

/// A 16-bit PCM WAV file holding `frames` of `channels`, written by hand so
/// the test needs no encoder.
fn write_wav(path: &Path, channels: u16, rate: u32, samples: &[i16]) {
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    bytes.extend_from_slice(&(channels * 2).to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write wav");
}

#[test]
fn a_stereo_wav_decodes_with_its_layout_and_rate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("track.wav");
    let samples: Vec<i16> = (0..2000).map(|i| (i * 8) as i16).collect();
    write_wav(&path, 2, 44_100, &samples);
    let decoded = decode_player_file(&path).expect("decodes");
    assert_eq!(decoded.channels, 2);
    assert_eq!(decoded.sample_rate, 44_100);
    assert_eq!(decoded.samples.len(), 2000);
    assert!((decoded.samples[1] - 8.0 / 32768.0).abs() < 1e-6);
}

#[test]
fn a_mono_wav_keeps_one_channel() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mono.wav");
    write_wav(&path, 1, 48_000, &[0, 16_384, -16_384]);
    let decoded = decode_player_file(&path).expect("decodes");
    assert_eq!(decoded.channels, 1);
    assert_eq!(decoded.sample_rate, 48_000);
    assert_eq!(decoded.samples.len(), 3);
    assert!((decoded.samples[2] + 0.5).abs() < 1e-6);
}

#[test]
fn a_missing_file_is_an_error() {
    assert!(decode_player_file(Path::new("/definitely/not/here.wav")).is_err());
}

#[test]
fn a_file_that_is_not_audio_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fake.mp3");
    std::fs::write(&path, b"this is not audio at all").unwrap();
    assert!(decode_player_file(&path).is_err());
}
