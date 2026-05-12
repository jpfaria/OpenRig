//! Chain-level loudness probe — calcula um gain ÚNICO em dB para que
//! qualquer combinação de blocos na chain saia no mesmo peak target,
//! independente de quantos pedais/amps estão ativos.
//!
//! Por que não somar manifest gains por bloco: cada gain do manifest
//! foi calibrado isoladamente (assumindo input cru ~-25 dBFS). Em
//! série eles empilham (Klon +28 + Amp +35 = +63 dB no signal) e
//! ultrapassam o teto. Esse módulo trata a chain como uma caixa
//! preta, mede o output real, e devolve a compensação única.
//!
//! Tudo OFFLINE — roda em thread isolada quando a chain é
//! construída/editada, NUNCA no audio thread. Resultado é uma
//! constante em dB que o runtime aplica como multiplicação simples.
//! Zero overhead per-sample, zero violação do CLAUDE.md.
//!
//! Pattern espelha `probe.rs` (latency probe): runtime temporário,
//! signal sintético, medição local, drop.

use crate::runtime::{
    build_chain_runtime_state, process_input_f32, process_output_f32, DEFAULT_ELASTIC_TARGET,
};
use project::chain::Chain;
use std::sync::Arc;

/// Peak alvo em dBFS após o gain de normalização. -1 dBFS deixa 1 dB
/// de margem do teto digital — chain sai forte mas sem clipar.
const TARGET_PEAK_DBFS: f32 = -1.0;

/// Quantos frames a chain processa por buffer no probe. Mesmo
/// tamanho típico de callback de áudio — mantém a chain no regime
/// que ela vai operar em produção.
const PROBE_BUFFER_FRAMES: usize = 256;

/// Estéreo. Chains estéreo precisam ver signal em ambos os canais
/// senão alguns processors (NAM, por exemplo) tomam atalho em
/// silêncio e a medição não reflete o caminho real.
const PROBE_CHANNELS: usize = 2;

/// Duração total do probe em segundos. 2 s é mais que suficiente:
/// pink noise é estacionário, o peak converge em poucas centenas
/// de ms, sobrando margem pra estabilização de blocos com filtro
/// (delay/reverb).
const PROBE_SECONDS: f32 = 2.0;

/// Peak do signal injetado, em dBFS. -25 dBFS é o nível típico de
/// guitarra de Scarlett-like line-in — o regime onde a chain vai
/// operar de verdade. Probe acima disso (DI denso a -12) faz a
/// chain calcular gain pra um cenário que não existe.
const PROBE_INPUT_PEAK_DBFS: f32 = -25.0;

/// Cap de boost. Capturas quietas (preamps trainados em -50 dBFS)
/// podem demandar 30+ dB. Acima disso o gain só amplifica noise
/// floor — vale clipar a normalização do que distorcer com ruído.
const MAX_GAIN_DB: f32 = 30.0;

/// Mínimo do gain. Boost-only por default: se a chain já está
/// acima do target, deixa quieta (não atenua — atenuar joga
/// loudness no lixo).
const MIN_GAIN_DB: f32 = 0.0;

const PROBE_SEED: u64 = 0xC0FFEE;

/// Build a temporary runtime, run pink noise through `chain`, and
/// return the dB gain that lands the output peak at [`TARGET_PEAK_DBFS`].
///
/// Returns `0.0` (no gain) on any build failure — chain stays at
/// its natural level rather than getting a wrong compensation.
pub fn compute_chain_normalization_gain_db(chain: &Chain, sample_rate: f32) -> f32 {
    let runtime = match build_chain_runtime_state(chain, sample_rate, &[DEFAULT_ELASTIC_TARGET]) {
        Ok(rt) => Arc::new(rt),
        Err(_) => return 0.0,
    };

    let total_frames = (sample_rate * PROBE_SECONDS) as usize;
    let num_iters = total_frames / PROBE_BUFFER_FRAMES;
    let mut input = vec![0.0_f32; PROBE_BUFFER_FRAMES * PROBE_CHANNELS];
    let mut output = vec![0.0_f32; PROBE_BUFFER_FRAMES * PROBE_CHANNELS];

    let mut rng = XorShift64::new(PROBE_SEED);
    let target_lin = 10f32.powf(PROBE_INPUT_PEAK_DBFS / 20.0);

    let mut max_peak: f32 = 0.0;

    for _ in 0..num_iters {
        for sample in input.iter_mut() {
            *sample = rng.next_f32_signed() * target_lin;
        }
        process_input_f32(&runtime, 0, &input, PROBE_CHANNELS);
        process_output_f32(&runtime, 0, &mut output, PROBE_CHANNELS);

        for s in output.iter() {
            let abs = s.abs();
            if abs > max_peak {
                max_peak = abs;
            }
        }
    }

    if max_peak < 1e-12 {
        return 0.0;
    }
    let measured_peak_dbfs = 20.0 * max_peak.log10();
    let raw_gain = TARGET_PEAK_DBFS - measured_peak_dbfs;
    raw_gain.clamp(MIN_GAIN_DB, MAX_GAIN_DB)
}

/// dB-to-linear helper for the audio thread side. Inline + branchless
/// so the caller can plug it in a hot loop without ceremony.
#[inline]
pub fn gain_db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0xDEAD_BEEF } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_f32_signed(&mut self) -> f32 {
        (self.next_u64() as f64 / u64::MAX as f64) as f32 * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_db_to_lin_round_trips_zero() {
        assert!((gain_db_to_lin(0.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn gain_db_to_lin_six_db_is_two_x() {
        // Standard EE identity: +6 dB ≈ 2× linear.
        assert!((gain_db_to_lin(6.0) - 1.9953).abs() < 0.01);
    }
}
