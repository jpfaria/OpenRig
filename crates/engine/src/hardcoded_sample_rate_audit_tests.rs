//! Audit test: detect hardcoded sample rate assumptions (issue #723)
//! 
//! This test ensures that production code doesn't hardcode 48000/44100
//! where the live device sample_rate should be used instead.

#[cfg(test)]
mod hardcoded_sample_rate_bugs {
    /// Audit: probe.rs should use the sample_rate argument passed to measure_chain_dsp_latency_ms,
    /// not override it with a hardcoded nominal_sr = 48_000.0
    #[test]
    fn probe_measure_chain_latency_respects_device_rate() {
        // This test will fail if probe.rs's measure_chain_dsp_latency_ms
        // ignores its sample_rate argument and always uses 48000 internally.
        // After the audit reports bugs, we'll write deterministic assertions.
        panic!("RED-FIRST: audit needed — probe.rs:57 has nominal_sr = 48_000.0. Does measure_chain_dsp_latency_ms actually USE its sample_rate arg?");
    }

    /// Audit: desktop_app.rs:169 unwrap_or(48_000) rate needs classification
    #[test]
    fn desktop_app_unwrap_or_48k_must_be_classified() {
        panic!("RED-FIRST: audit — what does .unwrap_or(48_000) on line 169 do? Is it a device fallback or live audio path?");
    }

    /// Audit: EQ visualization sample rate hardcoding
    #[test]
    fn eq_visualization_must_use_live_device_rate() {
        panic!("RED-FIRST: audit — EQ_VIZ_SAMPLE_RATE = 48_000.0 at eq.rs:165. Is the EQ curve visualization computed at live device rate or fixed 48k?");
    }

    /// Audit: runtime.rs:133 hardcoded sr = 48_000.0
    #[test]
    fn runtime_48k_fallback_must_be_classified() {
        panic!("RED-FIRST: audit — runtime.rs:133 has sr = 48_000.0. Is this a safe pre-device default or a bug in the live audio path?");
    }

    /// Audit: search for meter/spectrum sessions reading device input without negotiated rate
    #[test]
    fn meter_session_must_use_negotiated_device_rate() {
        panic!("RED-FIRST: audit — is there a meter_session or spectrum session that reads input taps and assumes a rate, like tuner_session did before #723?");
    }
}
