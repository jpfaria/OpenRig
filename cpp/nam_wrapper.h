#pragma once

#ifdef __cplusplus
extern "C" {
#endif

void* nam_create(const char* model_path_utf8);
void  nam_destroy(void* handle);
void  nam_process(void* handle, const float* input, float* output, int nframes);

#ifdef __cplusplus
}
#endif