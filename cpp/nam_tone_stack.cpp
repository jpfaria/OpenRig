#include "nam_tone_stack.h"

namespace openrig
{
BasicNamToneStack::BasicNamToneStack()
{
  Reset(_sample_rate);
}

void BasicNamToneStack::Reset(double sample_rate)
{
  _sample_rate = sample_rate;
  SetBass(_bass);
  SetMiddle(_middle);
  SetTreble(_treble);
}

void BasicNamToneStack::SetBass(double value)
{
  _bass = value;
  const double gain_db = 4.0 * (value - 5.0);
  recursive_linear_filter::BiquadParams params(_sample_rate, 150.0, 0.707, gain_db);
  _tone_bass.SetParams(params);
}

void BasicNamToneStack::SetMiddle(double value)
{
  _middle = value;
  const double gain_db = 3.0 * (value - 5.0);
  const double quality = gain_db < 0.0 ? 1.5 : 0.7;
  recursive_linear_filter::BiquadParams params(_sample_rate, 425.0, quality, gain_db);
  _tone_mid.SetParams(params);
}

void BasicNamToneStack::SetTreble(double value)
{
  _treble = value;
  const double gain_db = 2.0 * (value - 5.0);
  recursive_linear_filter::BiquadParams params(_sample_rate, 1800.0, 0.707, gain_db);
  _tone_treble.SetParams(params);
}

DSP_SAMPLE** BasicNamToneStack::Process(DSP_SAMPLE** inputs, int num_channels, int num_frames)
{
  auto** bass = _tone_bass.Process(inputs, static_cast<size_t>(num_channels), static_cast<size_t>(num_frames));
  auto** mid = _tone_mid.Process(bass, static_cast<size_t>(num_channels), static_cast<size_t>(num_frames));
  return _tone_treble.Process(mid, static_cast<size_t>(num_channels), static_cast<size_t>(num_frames));
}
} // namespace openrig
