use crate::host::Lv2Plugin;
use block_core::MonoProcessor;
use std::ffi::c_void;

/// Maximum block size for LV2 processing.
const MAX_BLOCK_SIZE: usize = 4096;

/// Audio processor wrapping a loaded LV2 plugin instance.
///
/// Uses block-based processing: `run(N)` is called with the full buffer
/// size, which is required for plugins that use FFT or other windowed
/// analysis (e.g., pitch correction, spectral effects).
pub struct Lv2Processor {
    plugin: Lv2Plugin,
    /// Audio input buffer — connected to the plugin's audio input port.
    in_buf: Box<[f32; MAX_BLOCK_SIZE]>,
    /// Audio output buffer — connected to the plugin's audio output port.
    out_buf: Box<[f32; MAX_BLOCK_SIZE]>,
    /// Control port values — kept alive and connected.
    control_values: Vec<f32>,
    /// Audio input port indices (stored for potential future reconnection).
    _audio_in_ports: Vec<usize>,
    /// Audio output port indices (stored for potential future reconnection).
    _audio_out_ports: Vec<usize>,
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
        let mut in_buf = Box::new([0.0f32; MAX_BLOCK_SIZE]);
        let mut out_buf = Box::new([0.0f32; MAX_BLOCK_SIZE]);
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
            _audio_in_ports: audio_in_ports.to_vec(),
            _audio_out_ports: audio_out_ports.to_vec(),
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
        let len = buffer.len().min(MAX_BLOCK_SIZE);

        // Copy input into the connected buffer
        self.in_buf[..len].copy_from_slice(&buffer[..len]);

        // Run the plugin on the full block at once
        self.plugin.run(len as u32);

        // Copy output back
        buffer[..len].copy_from_slice(&self.out_buf[..len]);
    }
}
