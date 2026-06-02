#include "nam_wrapper.h"

#include <filesystem>
#include <cmath>
#include <memory>
#include <vector>

#include "../deps/NeuralAmpModelerCore/NAM/get_dsp.h"
#include "../deps/NeuralAmpModelerCore/NAM/dsp.h"
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
    h->noise_gate_threshold_db = config->noise_gate_threshold_db;
    h->noise_gate_enabled = config->noise_gate_enabled != 0;
    h->eq_enabled = config->eq_enabled != 0;
    h->eq_is_neutral =
      std::abs(config->bass - 5.0f) < 1.0e-6f &&
      std::abs(config->middle - 5.0f) < 1.0e-6f &&
      std::abs(config->treble - 5.0f) < 1.0e-6f;
    h->ir_enabled = config->ir_enabled != 0;
    h->dsp->Reset(sample_rate, 4096);
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

void nam_process(void* handle, const float* input, float* output, int nframes) {
  auto* h = reinterpret_cast<NamHandle*>(handle);
  if (!h || !h->dsp || !input || !output || nframes <= 0) {
    return;
  }

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
