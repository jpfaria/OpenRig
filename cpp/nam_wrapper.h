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
  // Issue #612: when non-zero, the catalog loudness audit already owns
  // the output level (it is baked into output_db), so the model's own
  // GetLoudness() normalization must be SUPPRESSED here to avoid
  // double-counting. When zero (legacy / non-audited model), the
  // wrapper normalizes the model toward its calibrated level so a
  // nonlinear NAM is driven the way it was trained instead of unity
  // (the "abafado" fix).
  unsigned char audit_overrides_baked_output;
} NamPluginConfig;

void* nam_create(const NamPluginConfig* config);
void  nam_destroy(void* handle);
void  nam_process(void* handle, const float* input, float* output, int nframes);

#ifdef __cplusplus
}
#endif
