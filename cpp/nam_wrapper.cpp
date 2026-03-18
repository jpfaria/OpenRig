#include "nam_wrapper.h"

#include <filesystem>
#include <memory>
#include <vector>

#include "../NeuralAmpModelerCore/NAM/get_dsp.h"
#include "../NeuralAmpModelerCore/NAM/dsp.h"

struct NamHandle {
  std::unique_ptr<nam::DSP> dsp;
};

void* nam_create(const char* model_path_utf8) {
  try {
    auto path = std::filesystem::u8path(model_path_utf8);
    auto model = nam::get_dsp(path);

    auto* h = new NamHandle();
    h->dsp = std::move(model);
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

  std::vector<NAM_SAMPLE> input_buffer(nframes);
  std::vector<NAM_SAMPLE> output_buffer(nframes, 0.0);

  for (int i = 0; i < nframes; ++i) {
    input_buffer[i] = static_cast<NAM_SAMPLE>(input[i]);
  }

  NAM_SAMPLE* input_channels[] = { input_buffer.data() };
  NAM_SAMPLE* output_channels[] = { output_buffer.data() };

  h->dsp->process(input_channels, output_channels, nframes);

  for (int i = 0; i < nframes; ++i) {
    output[i] = static_cast<float>(output_buffer[i]);
  }
}