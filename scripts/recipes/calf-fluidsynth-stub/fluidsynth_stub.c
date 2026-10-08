/* No-op FluidSynth implementation: every call succeeds and renders silence. */
#include "fluidsynth.h"

static char settings_token, synth_token;

fluid_settings_t *new_fluid_settings(void) { return (fluid_settings_t *)&settings_token; }
void delete_fluid_settings(fluid_settings_t *settings) { (void)settings; }
int fluid_settings_setint(fluid_settings_t *s, const char *n, int v) { (void)s; (void)n; (void)v; return 0; }
int fluid_settings_setnum(fluid_settings_t *s, const char *n, double v) { (void)s; (void)n; (void)v; return 0; }
fluid_synth_t *new_fluid_synth(fluid_settings_t *s) { (void)s; return (fluid_synth_t *)&synth_token; }
void delete_fluid_synth(fluid_synth_t *synth) { (void)synth; }
int fluid_synth_sfload(fluid_synth_t *s, const char *f, int r) { (void)s; (void)f; (void)r; return 1; }
int fluid_synth_program_select(fluid_synth_t *s, int c, int id, int b, int p) { (void)s; (void)c; (void)id; (void)b; (void)p; return 0; }
int fluid_synth_noteon(fluid_synth_t *s, int c, int k, int v) { (void)s; (void)c; (void)k; (void)v; return 0; }
int fluid_synth_noteoff(fluid_synth_t *s, int c, int k) { (void)s; (void)c; (void)k; return 0; }
int fluid_synth_pitch_bend(fluid_synth_t *s, int c, int v) { (void)s; (void)c; (void)v; return 0; }
int fluid_synth_pitch_wheel_sens(fluid_synth_t *s, int c, int v) { (void)s; (void)c; (void)v; return 0; }
void fluid_synth_set_gain(fluid_synth_t *s, float g) { (void)s; (void)g; }
int fluid_synth_set_gen(fluid_synth_t *s, int c, int p, float v) { (void)s; (void)c; (void)p; (void)v; return 0; }

int fluid_synth_write_float(fluid_synth_t *s, int len, void *lout, int loff, int lincr,
                            void *rout, int roff, int rincr)
{
    float *l = (float *)lout, *r = (float *)rout;
    int i;
    (void)s;
    for (i = 0; i < len; i++) {
        l[loff + i * lincr] = 0.0f;
        r[roff + i * rincr] = 0.0f;
    }
    return 0;
}
