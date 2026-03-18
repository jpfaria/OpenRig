#pragma once

#include "../NeuralAmpModelerCore/Dependencies/AudioDSPTools/dsp/RecursiveLinearFilter.h"
#include "../NeuralAmpModelerCore/Dependencies/AudioDSPTools/dsp/dsp.h"

namespace openrig
{
class BasicNamToneStack
{
public:
  BasicNamToneStack();

  void Reset(double sample_rate);
  void SetBass(double value);
  void SetMiddle(double value);
  void SetTreble(double value);
  DSP_SAMPLE** Process(DSP_SAMPLE** inputs, int num_channels, int num_frames);

private:
  double _sample_rate = 48000.0;
  double _bass = 5.0;
  double _middle = 5.0;
  double _treble = 5.0;
  recursive_linear_filter::LowShelf _tone_bass;
  recursive_linear_filter::Peaking _tone_mid;
  recursive_linear_filter::HighShelf _tone_treble;
};
} // namespace openrig
