//! Responsibility: resolves the sample rate every side of a chain agrees on.

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use anyhow::bail;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use anyhow::{anyhow, Result};
#[cfg(all(test, not(all(target_os = "linux", feature = "jack"))))]
use cpal::SupportedStreamConfig;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::resolved::ResolvedInputDevice;
use crate::resolved::ResolvedOutputDevice;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn resolved_input_sample_rate(resolved: &ResolvedInputDevice) -> u32 {
    resolved
        .settings
        .as_ref()
        .map(|settings| settings.sample_rate)
        .unwrap_or_else(|| resolved.supported.sample_rate())
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn resolved_output_sample_rate(resolved: &ResolvedOutputDevice) -> u32 {
    resolved
        .settings
        .as_ref()
        .map(|settings| settings.sample_rate)
        .unwrap_or_else(|| resolved.supported.sample_rate())
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn resolved_input_buffer_size_frames(resolved: &ResolvedInputDevice) -> u32 {
    resolved
        .settings
        .as_ref()
        .map(|settings| settings.buffer_size_frames)
        .unwrap_or(256)
}

pub(crate) fn resolved_output_buffer_size_frames(resolved: &ResolvedOutputDevice) -> u32 {
    resolved
        .settings
        .as_ref()
        .map(|settings| settings.buffer_size_frames)
        .unwrap_or(256)
}

#[cfg(all(test, not(all(target_os = "linux", feature = "jack"))))]
pub(crate) fn resolve_chain_runtime_sample_rate(
    chain_id: &str,
    input: &SupportedStreamConfig,
    output: &SupportedStreamConfig,
) -> Result<f32> {
    if input.sample_rate() != output.sample_rate() {
        bail!(
            "chain '{}' invalid: input sample_rate={} differs from output sample_rate={}",
            chain_id,
            input.sample_rate(),
            output.sample_rate()
        );
    }

    Ok(input.sample_rate() as f32)
}

/// Pure unification of the resolved per-device rates: every input and every
/// output of one chain must agree (one engine clock). Returns the agreed rate
/// or a precise error naming whether the disagreement is input↔input or
/// input↔output. Pure — no hardware — so it is directly unit-testable.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn unify_io_sample_rates(
    chain_id: &str,
    input_rates: &[u32],
    output_rates: &[u32],
) -> Result<f32> {
    let mut rate: Option<u32> = None;
    for &sr in input_rates {
        if let Some(prev) = rate {
            if prev != sr {
                bail!(
                    "chain '{}' invalid: mismatched sample rates across inputs ({} vs {})",
                    chain_id,
                    prev,
                    sr
                );
            }
        }
        rate = Some(sr);
    }
    for &sr in output_rates {
        if let Some(prev) = rate {
            if prev != sr {
                bail!(
                    "chain '{}' invalid: mismatched sample rates across I/O ({} vs {})",
                    chain_id,
                    prev,
                    sr
                );
            }
        }
        rate = Some(sr);
    }
    rate.map(|r| r as f32)
        .ok_or_else(|| anyhow!("chain '{}' has no inputs or outputs", chain_id))
}

/// Resolve the sample rate **per binding-group** instead of once for the whole
/// chain (#736). Each `(input_rates, output_rates)` tuple is one I/O binding's
/// device rates. Within a binding, every input and output must agree (one
/// isolated stream needs no internal resample) — reuses `unify_io_sample_rates`,
/// so the within-binding error wording is unchanged ("across inputs" / "across
/// I/O"). Across bindings, rates may DIFFER freely — that is the whole point of
/// invariant #4 (stream isolation): two isolated streams share no clock.
///
/// Returns the FIRST binding's rate as the chain's representative scalar (used
/// for legacy single-rate consumers: stream-signature back-compat, DI-loop
/// resample target). The authoritative per-device rates flow separately through
/// `ResolvedChainAudioConfig::by_device`. With a single binding this equals the
/// legacy whole-chain `unify_io_sample_rates` result — single-binding chains are
/// bit-identical.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn resolve_binding_sample_rates(
    chain_id: &str,
    bindings: &[(Vec<u32>, Vec<u32>)],
) -> Result<f32> {
    let mut representative: Option<f32> = None;
    for (input_rates, output_rates) in bindings {
        let rate = unify_io_sample_rates(chain_id, input_rates, output_rates)?;
        if representative.is_none() {
            representative = Some(rate);
        }
    }
    representative.ok_or_else(|| anyhow!("chain '{}' has no inputs or outputs", chain_id))
}

#[cfg(all(test, not(all(target_os = "linux", feature = "jack"))))]
mod unify_rate_tests {
    use super::unify_io_sample_rates;

    #[test]
    fn agreeing_rates_resolve_to_that_rate() {
        assert_eq!(
            unify_io_sample_rates("c", &[44_100, 44_100], &[44_100]).unwrap(),
            44_100.0
        );
    }

    #[test]
    fn no_io_is_an_error() {
        assert!(unify_io_sample_rates("c", &[], &[]).is_err());
    }

    #[test]
    fn mismatched_inputs_error_names_inputs() {
        let e = unify_io_sample_rates("c", &[48_000, 44_100], &[48_000])
            .unwrap_err()
            .to_string();
        assert!(e.contains("across inputs"), "got: {e}");
    }

    #[test]
    fn mismatched_input_vs_output_error_names_io() {
        // Inputs agree (44.1k) but the output is 48k — the exact #669/#698
        // crackle shape (engine clock disagreeing with the device). Must be a
        // loud error, never a silent resample.
        let e = unify_io_sample_rates("c", &[44_100], &[48_000])
            .unwrap_err()
            .to_string();
        assert!(e.contains("across I/O"), "got: {e}");
    }

    #[test]
    fn single_output_only_resolves() {
        assert_eq!(
            unify_io_sample_rates("c", &[], &[48_000]).unwrap(),
            48_000.0
        );
    }

    use super::resolve_binding_sample_rates;

    #[test]
    fn two_bindings_at_different_rates_resolve_without_error() {
        // Scarlett binding @44.1k, TEYUN binding @48k — the #736 case.
        // Cross-binding difference is allowed; representative = first binding.
        let rate = resolve_binding_sample_rates(
            "c",
            &[(vec![44_100], vec![44_100]), (vec![48_000], vec![48_000])],
        )
        .expect("cross-binding rate difference must be allowed");
        assert_eq!(rate, 44_100.0);
    }

    #[test]
    fn within_binding_input_output_mismatch_still_errors() {
        // 44.1k input + 48k output INSIDE one binding — one isolated stream
        // cannot internally resample, so this stays a loud error (#669 shape).
        let e = resolve_binding_sample_rates("c", &[(vec![44_100], vec![48_000])])
            .unwrap_err()
            .to_string();
        assert!(e.contains("across I/O"), "got: {e}");
    }

    #[test]
    fn single_binding_matches_legacy_unify() {
        // One binding with all the chain's I/O → identical to whole-chain unify.
        let rate =
            resolve_binding_sample_rates("c", &[(vec![44_100, 44_100], vec![44_100])]).unwrap();
        assert_eq!(rate, 44_100.0);
    }

    #[test]
    fn no_bindings_is_an_error() {
        assert!(resolve_binding_sample_rates("c", &[]).is_err());
    }
}
