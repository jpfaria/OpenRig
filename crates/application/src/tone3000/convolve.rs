//! Responsibility: convolves a signal with an impulse response offline.
//!
//! One zero-padded FFT, as in OpenRig-plugins `loudness_audit`: the DI and
//! an IR are short enough for a single transform.

use realfft::RealFftPlanner;

/// Full linear convolution; output length is `sig + ir - 1`.
pub fn convolve(sig: &[f32], ir: &[f32]) -> Vec<f32> {
    if sig.is_empty() || ir.is_empty() {
        return Vec::new();
    }
    let n_lin = sig.len() + ir.len() - 1;
    let n = n_lin.next_power_of_two();
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let ifft = planner.plan_fft_inverse(n);

    let mut a = fft.make_input_vec();
    let mut b = fft.make_input_vec();
    a[..sig.len()].copy_from_slice(sig);
    b[..ir.len()].copy_from_slice(ir);

    let mut sa = fft.make_output_vec();
    let mut sb = fft.make_output_vec();
    // Buffers come from the planner, so their lengths always match.
    fft.process(&mut a, &mut sa)
        .expect("forward FFT of the signal");
    fft.process(&mut b, &mut sb).expect("forward FFT of the IR");
    for (x, y) in sa.iter_mut().zip(sb.iter()) {
        *x *= *y;
    }
    let mut out = ifft.make_output_vec();
    ifft.process(&mut sa, &mut out).expect("inverse FFT");

    let scale = 1.0 / n as f32;
    out.truncate(n_lin);
    out.iter_mut().for_each(|v| *v *= scale);
    out
}
