#pragma once

#ifdef __cplusplus
extern "C" {
#endif

typedef struct NamPluginConfig {
  const char* model_path_utf8;
  const char* ir_path_utf8;
  float input_db;
  float output_db;
  float noise_gate_threshold_db;
  float bass;
  float middle;
  float treble;
  unsigned char noise_gate_enabled;
  unsigned char eq_enabled;
  unsigned char ir_enabled;
} NamPluginConfig;

void* nam_create(const NamPluginConfig* config);
void  nam_destroy(void* handle);
void  nam_process(void* handle, const float* input, float* output, int nframes);

#ifdef __cplusplus
}
#endif
