use crate::host::Lv2Plugin;
use block_core::StereoProcessor;
use std::ffi::c_void;

/// Stereo audio processor wrapping a loaded LV2 plugin with 2-in/2-out audio.
///
/// Unlike `Lv2Processor` (mono), this connects separate L/R buffers.

/// Maximum block size for LV2 processing (matches `Lv2Processor`).
const MAX_BLOCK_SIZE: usize = 4096;

/// Size of the dummy atom buffer for MIDI/atom sidechain ports.
const ATOM_BUF_SIZE: usize = 256;

pub struct StereoLv2Processor {
    plugin: Lv2Plugin,
    in_buf_l: Box<[f32; MAX_BLOCK_SIZE]>,
    in_buf_r: Box<[f32; MAX_BLOCK_SIZE]>,
    out_buf_l: Box<[f32; MAX_BLOCK_SIZE]>,
    out_buf_r: Box<[f32; MAX_BLOCK_SIZE]>,
    control_values: Vec<f32>,
    _atom_buf: Box<[u8; ATOM_BUF_SIZE]>,
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
        Self::with_atom_ports(plugin, audio_in_ports, audio_out_ports, control_ports, &[])
    }

    pub fn with_atom_ports(
        plugin: Lv2Plugin,
        audio_in_ports: &[usize],
        audio_out_ports: &[usize],
        control_ports: &[(usize, f32)],
        atom_ports: &[usize],
    ) -> Self {
        assert!(audio_in_ports.len() == 2, "stereo requires 2 audio inputs");
        assert!(audio_out_ports.len() == 2, "stereo requires 2 audio outputs");

        let mut in_buf_l = Box::new([0.0f32; MAX_BLOCK_SIZE]);
        let mut in_buf_r = Box::new([0.0f32; MAX_BLOCK_SIZE]);
        let mut out_buf_l = Box::new([0.0f32; MAX_BLOCK_SIZE]);
        let mut out_buf_r = Box::new([0.0f32; MAX_BLOCK_SIZE]);
        let mut control_values: Vec<f32> = control_ports.iter().map(|(_, v)| *v).collect();

        let mut atom_buf = Box::new([0u8; ATOM_BUF_SIZE]);
        atom_buf[0] = 8; // atom.size = 8

        for &port_idx in atom_ports {
            unsafe {
                plugin.connect_port(port_idx as u32, atom_buf.as_mut_ptr() as *mut c_void);
            }
        }

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
            _atom_buf: atom_buf,
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
        let len = buffer.len().min(MAX_BLOCK_SIZE);

        for (i, frame) in buffer[..len].iter().enumerate() {
            self.in_buf_l[i] = frame[0];
            self.in_buf_r[i] = frame[1];
        }

        self.plugin.run(len as u32);

        for (i, frame) in buffer[..len].iter_mut().enumerate() {
            frame[0] = self.out_buf_l[i];
            frame[1] = self.out_buf_r[i];
        }
    }
}
