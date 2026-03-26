use crate::host::Lv2Plugin;
use block_core::StereoProcessor;
use std::ffi::c_void;

/// Stereo audio processor wrapping a loaded LV2 plugin with 2-in/2-out audio.
///
/// Unlike `Lv2Processor` (mono), this connects separate L/R buffers.
pub struct StereoLv2Processor {
    plugin: Lv2Plugin,
    in_buf_l: Box<[f32; 1]>,
    in_buf_r: Box<[f32; 1]>,
    out_buf_l: Box<[f32; 1]>,
    out_buf_r: Box<[f32; 1]>,
    control_values: Vec<f32>,
}

impl StereoLv2Processor {
    /// Create a new stereo processor.
    ///
    /// - `audio_in_ports`: exactly 2 port indices `[left_in, right_in]`
    /// - `audio_out_ports`: exactly 2 port indices `[left_out, right_out]`
    /// - `control_ports`: `(port_index, initial_value)` pairs
    pub fn new(
        plugin: Lv2Plugin,
        audio_in_ports: &[usize],
        audio_out_ports: &[usize],
        control_ports: &[(usize, f32)],
    ) -> Self {
        assert!(audio_in_ports.len() == 2, "stereo requires 2 audio inputs");
        assert!(audio_out_ports.len() == 2, "stereo requires 2 audio outputs");

        let mut in_buf_l = Box::new([0.0f32; 1]);
        let mut in_buf_r = Box::new([0.0f32; 1]);
        let mut out_buf_l = Box::new([0.0f32; 1]);
        let mut out_buf_r = Box::new([0.0f32; 1]);
        let mut control_values: Vec<f32> = control_ports.iter().map(|(_, v)| *v).collect();

        unsafe {
            plugin.connect_port(audio_in_ports[0] as u32, in_buf_l.as_mut_ptr() as *mut c_void);
            plugin.connect_port(audio_in_ports[1] as u32, in_buf_r.as_mut_ptr() as *mut c_void);
            plugin.connect_port(audio_out_ports[0] as u32, out_buf_l.as_mut_ptr() as *mut c_void);
            plugin.connect_port(audio_out_ports[1] as u32, out_buf_r.as_mut_ptr() as *mut c_void);
        }

        for (i, (port_idx, _)) in control_ports.iter().enumerate() {
            unsafe {
                plugin.connect_port(
                    *port_idx as u32,
                    &mut control_values[i] as *mut f32 as *mut c_void,
                );
            }
        }

        Self {
            plugin,
            in_buf_l,
            in_buf_r,
            out_buf_l,
            out_buf_r,
            control_values,
        }
    }

    pub fn set_control(&mut self, control_index: usize, value: f32) {
        if control_index < self.control_values.len() {
            self.control_values[control_index] = value;
        }
    }
}

impl StereoProcessor for StereoLv2Processor {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.in_buf_l[0] = input[0];
        self.in_buf_r[0] = input[1];
        self.plugin.run(1);
        [self.out_buf_l[0], self.out_buf_r[0]]
    }

    fn process_block(&mut self, buffer: &mut [[f32; 2]]) {
        for frame in buffer.iter_mut() {
            self.in_buf_l[0] = frame[0];
            self.in_buf_r[0] = frame[1];
            self.plugin.run(1);
            frame[0] = self.out_buf_l[0];
            frame[1] = self.out_buf_r[0];
        }
    }
}
