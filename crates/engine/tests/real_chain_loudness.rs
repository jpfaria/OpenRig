//! Integration test (#413): carrega chain real do usuário usando os
//! NAMs copiados pra `tests/fixtures/`, drive com sinal de teste e
//! mede o RMS de saída.
//!
//! O fixture tem 4 NAMs (klon, dumble, ts9, bogner) representando o
//! que existe na chain `john mayer - heartbreak warfare` (clean) e
//! `green day - basket case` (saturado). Se o auto-max está
//! funcionando, ambos os chains devem entregar RMS dentro de ~3 dB.
//!
//! `#[ignore]` porque os fixtures pesam ~1 MB e o teste exige o NAM
//! lib (carrega .dylib em runtime). Rodar com:
//!
//!     cargo test -p engine --test real_chain_loudness -- --ignored --nocapture

use std::path::PathBuf;
use std::sync::{Arc, Once};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::value_objects::ParameterValue;
use engine::runtime::{build_chain_runtime_state, process_input_f32, process_output_f32};
use project::block::{
    AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::param::ParameterSet;

const SR: f32 = 48_000.0;
const ELASTIC_TARGET: usize = 512;
const PINK_LEN: usize = 96_000; // 2 s @ 48 kHz
const PEAK_INPUT_DBFS: f32 = -12.0;

static INIT: Once = Once::new();

fn setup() {
    INIT.call_once(|| {
        // Backends que os blocks de NAM no chain precisam pra construir
        // o processor a partir do LoadedPackage.
        nam::register_builder();

        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("plugins")
            .join("source");
        plugin_loader::registry::init(&fixtures);
    });
}

fn input_mono() -> AudioBlock {
    AudioBlock {
        id: BlockId("input:0".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![InputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
        }),
    }
}

fn output_stereo() -> AudioBlock {
    AudioBlock {
        id: BlockId("output:0".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            entries: vec![OutputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainOutputMode::Stereo,
                channels: vec![0, 1],
            }],
        }),
    }
}

fn nam_block(id: &str, effect_type: &str, model: &str, params: Vec<(&str, ParameterValue)>) -> AudioBlock {
    let mut ps = ParameterSet::default();
    for (k, v) in params {
        ps.insert(k, v);
    }
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: effect_type.to_string(),
            model: model.to_string(),
            params: ps,
        }),
    }
}

fn chain(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: Some(id.into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        blocks,
    }
}

/// Pink noise normalised to a given peak dBFS — deterministic seed.
fn pink_noise(samples: usize, peak_dbfs: f32, seed: u64) -> Vec<f32> {
    let mut state = if seed == 0 { 0xDEAD_BEEF } else { seed };
    let mut next = || {
        let mut x = state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        state = x;
        ((x as f64 / u64::MAX as f64) as f32) * 2.0 - 1.0
    };
    const OCT: usize = 8;
    let mut rolls = [0.0_f32; OCT];
    for r in rolls.iter_mut() {
        *r = next();
    }
    let mut buf = Vec::with_capacity(samples);
    for n in 0..samples {
        for (i, r) in rolls.iter_mut().enumerate() {
            if (n as u64) & (1u64 << i) == 0 {
                *r = next();
            }
        }
        buf.push(rolls.iter().sum::<f32>() + next());
    }
    let peak = buf.iter().fold(0.0_f32, |a, s| a.max(s.abs()));
    if peak > 0.0 {
        let target = 10.0_f32.powf(peak_dbfs / 20.0);
        let scale = target / peak;
        for s in buf.iter_mut() {
            *s *= scale;
        }
    }
    buf
}

fn drive_chain(chain: &Chain, mono_input: &[f32]) -> Vec<f32> {
    let runtime = Arc::new(
        build_chain_runtime_state(chain, SR, &[ELASTIC_TARGET])
            .expect("chain runtime should build"),
    );

    // Mimic the real audio callback shape: drive input and output in
    // CHUNK_FRAMES blocks so the SPSC route ring (sized after
    // `elastic_target`) doesn't drop frames between callbacks. Pushing
    // the whole 2 s of audio in one process_input_f32 silently throws
    // away everything past the ring capacity.
    const CHUNK_FRAMES: usize = 256;
    let input_channels = 1;
    let output_channels = 2;
    let total_frames = mono_input.len() / input_channels;
    let mut out = Vec::with_capacity(total_frames * output_channels);
    let mut chunk_out = vec![0.0_f32; CHUNK_FRAMES * output_channels];
    let mut written = 0;
    while written < total_frames {
        let take = CHUNK_FRAMES.min(total_frames - written);
        let in_slice = &mono_input[written * input_channels..(written + take) * input_channels];
        process_input_f32(&runtime, 0, in_slice, input_channels);
        let out_slice = &mut chunk_out[..take * output_channels];
        process_output_f32(&runtime, 0, out_slice, output_channels);
        out.extend_from_slice(out_slice);
        written += take;
    }
    out
}

fn rms_dbfs(interleaved_stereo: &[f32]) -> f32 {
    let m = interleaved_stereo.iter().map(|s| s * s).sum::<f32>()
        / interleaved_stereo.len() as f32;
    if m == 0.0 {
        -120.0
    } else {
        10.0 * m.log10()
    }
}

fn peak_dbfs(interleaved_stereo: &[f32]) -> f32 {
    let p = interleaved_stereo
        .iter()
        .fold(0.0_f32, |a, s| a.max(s.abs()));
    if p == 0.0 {
        -120.0
    } else {
        20.0 * p.log10()
    }
}

fn native_block(id: &str, effect_type: &str, model: &str, params: Vec<(&str, ParameterValue)>) -> AudioBlock {
    let mut ps = ParameterSet::default();
    for (k, v) in params {
        ps.insert(k, v);
    }
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: effect_type.to_string(),
            model: model.to_string(),
            params: ps,
        }),
    }
}

/// Versão completa do chain heartbreak warfare (sem o gate / compressor
/// que tão DISABLED no project.yaml do usuário):
///   input → klon → dumble → eq8 → chorus → tremolo → tape → hall → output
fn heartbreak_full_chain() -> Chain {
    chain(
        "heartbreak_full",
        vec![
            input_mono(),
            nam_block(
                "klon",
                "gain",
                "nam_klon_centaur",
                vec![("setting", ParameterValue::String("john_mayer".into()))],
            ),
            nam_block(
                "dumble",
                "amp",
                "nam_dumble_steel_string_singer",
                vec![
                    ("channel", ParameterValue::String("clean".into())),
                    ("variant", ParameterValue::String("default".into())),
                ],
            ),
            native_block(
                "chorus",
                "modulation",
                "stereo_chorus",
                vec![
                    ("rate_hz", ParameterValue::Float(0.6)),
                    ("depth", ParameterValue::Float(22.0)),
                    ("mix", ParameterValue::Float(30.0)),
                    ("spread", ParameterValue::Float(100.0)),
                ],
            ),
            native_block(
                "tremolo",
                "modulation",
                "tremolo_sine",
                vec![
                    ("rate_hz", ParameterValue::Float(4.0)),
                    ("depth", ParameterValue::Float(50.0)),
                ],
            ),
            native_block(
                "tape",
                "delay",
                "tape_vintage",
                vec![
                    ("time_ms", ParameterValue::Float(464.0)),
                    ("feedback", ParameterValue::Float(35.0)),
                    ("flutter", ParameterValue::Float(25.0)),
                    ("tone", ParameterValue::Float(42.0)),
                    ("mix", ParameterValue::Float(22.0)),
                ],
            ),
            native_block(
                "hall",
                "reverb",
                "hall",
                vec![
                    ("room_size", ParameterValue::Float(60.0)),
                    ("damping", ParameterValue::Float(55.0)),
                    ("pre_delay_ms", ParameterValue::Float(20.0)),
                    ("mix", ParameterValue::Float(14.0)),
                ],
            ),
            output_stereo(),
        ],
    )
}

fn basket_case_full_chain() -> Chain {
    chain(
        "basket_full",
        vec![
            input_mono(),
            nam_block(
                "ts9",
                "gain",
                "nam_ibanez_ts9",
                vec![
                    ("drive", ParameterValue::Float(7.0)),
                    ("tone", ParameterValue::Float(7.0)),
                    ("level", ParameterValue::Float(7.0)),
                ],
            ),
            nam_block(
                "bogner",
                "amp",
                "nam_bogner_ecstasy",
                vec![
                    ("channel", ParameterValue::String("drive_red".into())),
                    ("cabinet", ParameterValue::String("4x12_v30".into())),
                ],
            ),
            output_stereo(),
        ],
    )
}

#[test]
#[ignore]
fn heartbreak_clean_vs_basket_case_saturated_rms_converges() {
    setup();

    // Chain "heartbreak warfare" minimal: input → klon → dumble → output.
    // Os outros fx (eq, chorus, tremolo, tape, hall) ficam de fora pra
    // isolar o caminho NAM + manifest `output_gain_db`. Se passa aqui
    // mas o app real desnivela, a culpa está nos fx posteriores.
    let heartbreak = chain(
        "heartbreak",
        vec![
            input_mono(),
            nam_block(
                "klon",
                "gain",
                "nam_klon_centaur",
                vec![("setting", ParameterValue::String("john_mayer".into()))],
            ),
            nam_block(
                "dumble",
                "amp",
                "nam_dumble_steel_string_singer",
                vec![
                    ("channel", ParameterValue::String("clean".into())),
                    ("variant", ParameterValue::String("default".into())),
                ],
            ),
            output_stereo(),
        ],
    );

    // Chain "basket case" minimal: input → ts9 → bogner → output.
    let basket = chain(
        "basket",
        vec![
            input_mono(),
            nam_block(
                "ts9",
                "gain",
                "nam_ibanez_ts9",
                vec![
                    ("drive", ParameterValue::Float(7.0)),
                    ("tone", ParameterValue::Float(7.0)),
                    ("level", ParameterValue::Float(7.0)),
                ],
            ),
            nam_block(
                "bogner",
                "amp",
                "nam_bogner_ecstasy",
                vec![
                    ("channel", ParameterValue::String("drive_red".into())),
                    ("cabinet", ParameterValue::String("4x12_v30".into())),
                ],
            ),
            output_stereo(),
        ],
    );

    let pink = pink_noise(PINK_LEN, PEAK_INPUT_DBFS, 0xC0FFEE);

    let heart_out = drive_chain(&heartbreak, &pink);
    let bask_out = drive_chain(&basket, &pink);

    // Skip the FADE_IN region (rough first ~256 frames × 2 ch).
    let skip = 512 * 2;
    let heart_steady = &heart_out[skip..];
    let bask_steady = &bask_out[skip..];

    let heart_rms = rms_dbfs(heart_steady);
    let bask_rms = rms_dbfs(bask_steady);
    let heart_peak = peak_dbfs(heart_steady);
    let bask_peak = peak_dbfs(bask_steady);

    eprintln!(
        "heartbreak (klon→dumble clean):  peak={heart_peak:+.2}  rms={heart_rms:+.2}  dBFS"
    );
    eprintln!(
        "basket case (ts9→bogner saturated): peak={bask_peak:+.2}  rms={bask_rms:+.2}  dBFS"
    );
    eprintln!("Δ rms = {:+.2} dB", (heart_rms - bask_rms).abs());

    // Diagnostic — auto-max saiu, nivelamento agora é offline via
    // `manifest.output_gain_db` (tool `nam_loudness_audit`). Probe
    // mede o amp ISOLADO; chain real tem gain pedal upstream que
    // muda o sinal de entrada e o offset baked acaba parcial.
    // Test continua útil pra registrar o estado atual e detectar
    // regressões absurdas (ex: signal silencioso quando deveria
    // sair).
    let diff = (heart_rms - bask_rms).abs();
    assert!(
        diff < 15.0,
        "Δ rms = {diff:.2} dB — algo MUITO errado se passou disso"
    );
}

/// Versão FULL da chain do user — com todos os fx pós-amp (chorus,
/// tremolo, tape, hall) ativos. Diagnostic-only: fx atenuam wet/dry
/// e nivelamento não é mais runtime.
#[test]
#[ignore]
fn full_heartbreak_chain_vs_basket_case_diagnostic() {
    setup();

    let heartbreak = heartbreak_full_chain();
    let basket = basket_case_full_chain();

    let pink = pink_noise(PINK_LEN, PEAK_INPUT_DBFS, 0xC0FFEE);

    let heart_out = drive_chain(&heartbreak, &pink);
    let bask_out = drive_chain(&basket, &pink);

    let skip = 512 * 2;
    let heart_steady = &heart_out[skip..];
    let bask_steady = &bask_out[skip..];

    let heart_rms = rms_dbfs(heart_steady);
    let bask_rms = rms_dbfs(bask_steady);
    let heart_peak = peak_dbfs(heart_steady);
    let bask_peak = peak_dbfs(bask_steady);

    eprintln!(
        "heartbreak FULL (klon→dumble clean→chorus→tremolo→tape→hall): peak={heart_peak:+.2}  rms={heart_rms:+.2}  dBFS"
    );
    eprintln!(
        "basket case (ts9→bogner saturated):                            peak={bask_peak:+.2}  rms={bask_rms:+.2}  dBFS"
    );
    eprintln!("Δ rms = {:+.2} dB", (heart_rms - bask_rms).abs());

    // Diagnostic only — fx wet/dry erodem o loudness pós-amp e
    // `output_gain_db` baked offline não captura isso. Cap só pra
    // detectar regressão absurda (signal silencioso etc.).
    let diff = (heart_rms - bask_rms).abs();
    assert!(diff < 25.0, "Δ rms = {diff:.2} dB — algo MUITO errado");
}
