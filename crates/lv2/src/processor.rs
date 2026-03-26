use crate::host::Lv2Plugin;
use block_core::MonoProcessor;
use std::ffi::c_void;

/// Audio processor wrapping a loaded LV2 plugin instance.
///
/// Ports are connected once at construction; `process_sample` simply writes
/// the input, calls `run(1)` and reads the output.
pub struct Lv2Processor {
    plugin: Lv2Plugin,
    // Scratch buffers — kept alive so the pointers stay valid.
    in_buf: Box<[f32; 1]>,
    out_buf: Box<[f32; 1]>,
    // Control port values — kept alive and connected.
    control_values: Vec<f32>,
}

impl Lv2Processor {
    /// Create a new processor.
    ///
    /// - `plugin`: an already-loaded LV2 plugin instance
    /// - `audio_in_ports`: port indices for audio inputs
    /// - `audio_out_ports`: port indices for audio outputs
    /// - `control_ports`: `(port_index, initial_value)` pairs
    ///
    /// Ports are connected immediately and remain connected for the lifetime
    /// of this struct (buffer addresses never change).
    pub fn new(
        plugin: Lv2Plugin,
        audio_in_ports: &[usize],
        audio_out_ports: &[usize],
        control_ports: &[(usize, f32)],
    ) -> Self {
        let mut in_buf = Box::new([0.0f32; 1]);
        let mut out_buf = Box::new([0.0f32; 1]);
        let mut control_values: Vec<f32> = control_ports.iter().map(|(_, v)| *v).collect();

        // Connect audio input ports
        for &port_idx in audio_in_ports {
            unsafe {
                plugin.connect_port(
                    port_idx as u32,
                    in_buf.as_mut_ptr() as *mut c_void,
                );
            }
        }

        // Connect audio output ports
        for &port_idx in audio_out_ports {
            unsafe {
                plugin.connect_port(
                    port_idx as u32,
                    out_buf.as_mut_ptr() as *mut c_void,
                );
            }
        }

        // Connect control ports
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
            in_buf,
            out_buf,
            control_values,
        }
    }

    /// Update a control port value by its position in the `control_ports`
    /// slice that was passed to `new()`.
    pub fn set_control(&mut self, control_index: usize, value: f32) {
        if control_index < self.control_values.len() {
            self.control_values[control_index] = value;
        }
    }
}

impl MonoProcessor for Lv2Processor {
    fn process_sample(&mut self, input: f32) -> f32 {
        self.in_buf[0] = input;
        self.plugin.run(1);
        self.out_buf[0]
    }

    fn process_block(&mut self, buffer: &mut [f32]) {
        // Sample-by-sample for v1; block optimization can come later.
        for sample in buffer.iter_mut() {
            self.in_buf[0] = *sample;
            self.plugin.run(1);
            *sample = self.out_buf[0];
        }
    }
}
