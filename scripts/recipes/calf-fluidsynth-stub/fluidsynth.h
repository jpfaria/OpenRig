/* No-op FluidSynth API used only to build Calf without the real library.
 * Calf links FluidSynth for the Vinyl module, which OpenRig does not ship. */
#ifndef OPENRIG_FLUIDSYNTH_STUB_H
#define OPENRIG_FLUIDSYNTH_STUB_H
#ifdef __cplusplus
extern "C" {
#endif
typedef struct _fluid_synth_t fluid_synth_t;
typedef struct _fluid_settings_t fluid_settings_t;
enum { GEN_ATTENUATION = 48 };
fluid_settings_t *new_fluid_settings(void);
void delete_fluid_settings(fluid_settings_t *settings);
int fluid_settings_setint(fluid_settings_t *settings, const char *name, int val);
int fluid_settings_setnum(fluid_settings_t *settings, const char *name, double val);
fluid_synth_t *new_fluid_synth(fluid_settings_t *settings);
void delete_fluid_synth(fluid_synth_t *synth);
int fluid_synth_sfload(fluid_synth_t *synth, const char *filename, int reset_presets);
int fluid_synth_program_select(fluid_synth_t *synth, int chan, int sfont_id, int bank, int preset);
int fluid_synth_noteon(fluid_synth_t *synth, int chan, int key, int vel);
int fluid_synth_noteoff(fluid_synth_t *synth, int chan, int key);
int fluid_synth_pitch_bend(fluid_synth_t *synth, int chan, int val);
int fluid_synth_pitch_wheel_sens(fluid_synth_t *synth, int chan, int val);
void fluid_synth_set_gain(fluid_synth_t *synth, float gain);
int fluid_synth_set_gen(fluid_synth_t *synth, int chan, int param, float value);
int fluid_synth_write_float(fluid_synth_t *synth, int len, void *lout, int loff, int lincr,
                            void *rout, int roff, int rincr);
#ifdef __cplusplus
}
#endif
#endif
