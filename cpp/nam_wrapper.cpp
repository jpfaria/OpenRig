#include "nam_wrapper.h"

#include <algorithm>
#include <filesystem>
#include <cmath>
#include <memory>
#include <vector>

#include "../deps/NeuralAmpModelerCore/NAM/get_dsp.h"
#include "../deps/NeuralAmpModelerCore/NAM/dsp.h"
#include "../deps/NeuralAmpModelerCore/NAM/slimmable.h"
#include "../deps/NeuralAmpModelerCore/Dependencies/AudioDSPTools/dsp/ImpulseResponse.h"
#include "../deps/NeuralAmpModelerCore/Dependencies/AudioDSPTools/dsp/NoiseGate.h"
#include "../deps/NeuralAmpModelerCore/Dependencies/AudioDSPTools/dsp/dsp.h"
#include "nam_tone_stack.h"

struct NamHandle {
  std::unique_ptr<nam::DSP> dsp;
  std::unique_ptr<dsp::ImpulseResponse> ir;
  dsp::noise_gate::Trigger noise_gate_trigger;
  dsp::noise_gate::Gain noise_gate_gain;
  openrig::BasicNamToneStack tone_stack;
  std::vector<double> ir_input_buffer;
  std::vector<double> input_buffer;
  std::vector<NAM_SAMPLE> model_input_buffer;
  std::vector<NAM_SAMPLE> model_raw_output_buffer;
  std::vector<double> model_output_buffer;
  double* ir_input_channels[1] = {nullptr};
  double* input_channels[1] = {nullptr};
  double* model_output_channels[1] = {nullptr};
  double input_gain = 1.0;
  double output_gain = 1.0;
  double sample_rate = 48000.0;
  double noise_gate_threshold_db = -80.0;
  bool noise_gate_enabled = true;
  bool eq_enabled = true;
  bool eq_is_neutral = true;
  bool ir_enabled = true;
};

namespace
{
double db_to_amp(const double db)
{
  return std::pow(10.0, db / 20.0);
}

// Issue #612: output loudness reference, in dB, for normalizing a model
// toward its calibrated level. NAM models carry their own measured
// loudness in metadata (nam::DSP::GetLoudness, in dB). Normalizing the
// output by `kLoudnessTargetDb - GetLoudness()` drives a nonlinear amp
// at the level it was trained at and matches the official
// NeuralAmpModeler plugin's output-normalization reference. Mirrors the
// per-model `recommended_output_db` the old neural-amp-modeler-lv2
// engine applied. Single source of truth for the reference.
constexpr double kLoudnessTargetDb = -18.0;

// Maximum frames passed to `nam::DSP::process` (and every other stage)
// in a single call. The model's internal buffers are sized by
// `Reset(sample_rate, kMaxBlock)`, so feeding more than this in one
// `process` call overruns them (SIGSEGV). Real-time callbacks are well
// under this, but offline harnesses (loudness/clip measurement) feed
// multi-second buffers in one shot, so `nam_process` chunks any input
// into slices of at most this many frames. Single source of truth for
// the create-time `Reset` and the process-time chunking.
constexpr int kMaxBlock = 4096;
}

void* nam_create(const NamPluginConfig* config) {
  try {
    if (config == nullptr || config->model_path_utf8 == nullptr || config->model_path_utf8[0] == '\0') {
      return nullptr;
    }

    auto path = std::filesystem::u8path(config->model_path_utf8);
    auto model = nam::get_dsp(path);

    auto* h = new NamHandle();
    h->dsp = std::move(model);
    const double sample_rate = h->dsp->GetExpectedSampleRate() > 0.0 ? h->dsp->GetExpectedSampleRate() : 48000.0;
    h->sample_rate = sample_rate;
    h->input_gain = db_to_amp(config->input_db);
    h->output_gain = db_to_amp(config->output_db);

    // Issue #612: fold the model's own calibration into the gain
    // staging so a nonlinear NAM is driven at the level it was trained
    // at, not raw unity (the "abafado / sem vida" fix). The user
    // input_db/output_db knobs stay additive on top (already applied
    // above). Suppressed when the catalog audit already owns the output
    // level (audit_overrides_baked_output), so the two never
    // double-count — this mirrors the old engine's gain_offsets
    // contract where the trainer recommendations were ignored once the
    // audit had run.
    if (config->audit_overrides_baked_output == 0) {
      // Output loudness normalization toward the reference. GetLoudness()
      // is in dB; the model is typically baked quiet (e.g. -23.98 dB), so
      // this is a boost up toward kLoudnessTargetDb.
      if (h->dsp->HasLoudness()) {
        const double output_norm_db = kLoudnessTargetDb - h->dsp->GetLoudness();
        h->output_gain *= db_to_amp(output_norm_db);
      }
      // Input calibration. GetInputLevel() is the dBu RMS the model
      // expects at 0 dBFS; SetInputLevel marks it present. The official
      // plugin drives the model at GetInputLevel() relative to the same
      // reference its output loudness was measured against, so the net
      // input-vs-output staging reproduces the trained operating point.
      if (h->dsp->HasInputLevel()) {
        const double input_cal_db = h->dsp->GetInputLevel() - kLoudnessTargetDb;
        h->input_gain *= db_to_amp(input_cal_db);
      }
    }
    h->noise_gate_threshold_db = config->noise_gate_threshold_db;
    h->noise_gate_enabled = config->noise_gate_enabled != 0;
    h->eq_enabled = config->eq_enabled != 0;
    h->eq_is_neutral =
      std::abs(config->bass - 5.0f) < 1.0e-6f &&
      std::abs(config->middle - 5.0f) < 1.0e-6f &&
      std::abs(config->treble - 5.0f) < 1.0e-6f;
    h->ir_enabled = config->ir_enabled != 0;
    h->dsp->Reset(sample_rate, kMaxBlock);
    // Issue #657: A2 SlimmableContainer models expose a runtime size
    // lever (nam::SlimmableModel::SetSlimmableSize, 0.0 smallest .. 1.0
    // full) that trades fidelity for CPU. A1 models do not implement the
    // interface, so the cast is null and the knob is inert. SetSlimmableSize
    // is thread-safe but NOT real-time safe, so it runs here at load (off
    // the audio thread), after Reset (it needs the configured sample rate
    // and buffer size); the staged submodel is then installed lock-free on
    // the first process() call. Calling it with the model's current size
    // (e.g. 1.0 = full, the post-construction state) is a no-op early-out
    // inside the container, so the default path is unchanged.
    if (auto* slimmable = dynamic_cast<nam::SlimmableModel*>(h->dsp.get())) {
      slimmable->SetSlimmableSize(static_cast<double>(config->slim_size));
    }
    h->noise_gate_trigger.AddListener(&h->noise_gate_gain);
    if (h->noise_gate_enabled) {
      const double time = 0.01;
      const double ratio = 0.1;
      const double open_time = 0.005;
      const double hold_time = 0.01;
      const double close_time = 0.05;
      dsp::noise_gate::TriggerParams params(
        time,
        h->noise_gate_threshold_db,
        ratio,
        open_time,
        hold_time,
        close_time
      );
      h->noise_gate_trigger.SetParams(params);
      h->noise_gate_trigger.SetSampleRate(sample_rate);
    }
    h->tone_stack.Reset(sample_rate);
    h->tone_stack.SetBass(config->bass);
    h->tone_stack.SetMiddle(config->middle);
    h->tone_stack.SetTreble(config->treble);
    if (config->ir_path_utf8 != nullptr && config->ir_path_utf8[0] != '\0') {
      const auto ir_path = std::filesystem::u8path(config->ir_path_utf8);
      auto ir = std::make_unique<dsp::ImpulseResponse>(ir_path.string().c_str(), sample_rate);
      if (ir->GetWavState() != dsp::wav::LoadReturnCode::SUCCESS) {
        delete h;
        return nullptr;
      }
      h->ir = std::move(ir);
    }
    return reinterpret_cast<void*>(h);
  } catch (...) {
    return nullptr;
  }
}

void nam_destroy(void* handle) {
  auto* h = reinterpret_cast<NamHandle*>(handle);
  delete h;
}

namespace
{
// Process a single block of at most `kMaxBlock` frames through the full
// chain: input gain → noise gate → model → gate → tone stack (EQ) → IR
// → output gain. `nam_process` slices oversized inputs and calls this
// per slice so the model's `Reset`-sized internal buffers are never
// overrun.
void nam_process_block(NamHandle* h, const float* input, float* output, int nframes) {
  h->input_buffer.resize(nframes);
  h->model_input_buffer.resize(nframes);
  h->model_raw_output_buffer.resize(nframes);
  h->model_output_buffer.resize(nframes);

  for (int i = 0; i < nframes; ++i) {
    h->input_buffer[i] = static_cast<double>(input[i]) * h->input_gain;
    h->model_input_buffer[i] = static_cast<NAM_SAMPLE>(h->input_buffer[i]);
  }

  double* trigger_input_channels[] = { h->input_buffer.data() };
  double** trigger_output = trigger_input_channels;
  if (h->noise_gate_enabled) {
    trigger_output = h->noise_gate_trigger.Process(trigger_input_channels, 1, static_cast<size_t>(nframes));
    for (int i = 0; i < nframes; ++i) {
      h->model_input_buffer[i] = static_cast<NAM_SAMPLE>(trigger_output[0][i]);
    }
  }

  NAM_SAMPLE* input_channels[] = { h->model_input_buffer.data() };
  NAM_SAMPLE* output_channels[] = { h->model_raw_output_buffer.data() };

  h->dsp->process(input_channels, output_channels, nframes);

  for (int i = 0; i < nframes; ++i) {
    h->model_output_buffer[i] = static_cast<double>(h->model_raw_output_buffer[i]);
  }

  h->model_output_channels[0] = h->model_output_buffer.data();
  double** post_nam_output = h->model_output_channels;
  if (h->noise_gate_enabled) {
    post_nam_output = h->noise_gate_gain.Process(h->model_output_channels, 1, static_cast<size_t>(nframes));
  }

  double** eq_output = post_nam_output;
  if (h->eq_enabled && !h->eq_is_neutral) {
    eq_output = h->tone_stack.Process(post_nam_output, 1, nframes);
  }

  if (h->ir && h->ir_enabled) {
    h->ir_input_buffer.resize(nframes);
    for (int i = 0; i < nframes; ++i) {
      h->ir_input_buffer[i] = eq_output[0][i];
    }
    h->ir_input_channels[0] = h->ir_input_buffer.data();
    auto** ir_output = h->ir->Process(h->ir_input_channels, 1, static_cast<size_t>(nframes));
    for (int i = 0; i < nframes; ++i) {
      output[i] = static_cast<float>(ir_output[0][i] * h->output_gain);
    }
  } else {
    for (int i = 0; i < nframes; ++i) {
      output[i] = static_cast<float>(eq_output[0][i] * h->output_gain);
    }
  }
}
} // namespace

void nam_process(void* handle, const float* input, float* output, int nframes) {
  auto* h = reinterpret_cast<NamHandle*>(handle);
  if (!h || !h->dsp || !input || !output || nframes <= 0) {
    return;
  }

  // Slice into blocks of at most kMaxBlock so the model's internal
  // buffers (sized by Reset(sr, kMaxBlock)) are never overrun. The gate
  // / EQ / IR are streaming and stateful, so processing the slices in
  // order is sample-for-sample equivalent to one big call (and is how a
  // real-time host already drives the chain, callback by callback).
  for (int offset = 0; offset < nframes; offset += kMaxBlock) {
    const int block = std::min(kMaxBlock, nframes - offset);
    nam_process_block(h, input + offset, output + offset, block);
  }
}
