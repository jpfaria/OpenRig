use crate::host::Lv2Plugin;
use block_core::MonoProcessor;
use std::ffi::c_void;

/// Maximum block size for LV2 processing.
const MAX_BLOCK_SIZE: usize = 4096;

/// Size of the dummy atom buffer for MIDI/atom sidechain ports.
/// Must be large enough for an empty LV2_Atom_Sequence header (16 bytes min).
const ATOM_BUF_SIZE: usize = 256;

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
    /// Dummy output buffer for extra output ports that must be connected but aren't read.
    _dummy_out_buf: Box<[f32; MAX_BLOCK_SIZE]>,
    /// Control port values — kept alive and connected.
    control_values: Vec<f32>,
    /// Dummy atom buffer for MIDI/atom sidechain ports.
    _atom_buf: Box<[u8; ATOM_BUF_SIZE]>,
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
        Self::with_atom_ports(plugin, audio_in_ports, audio_out_ports, control_ports, &[])
    }

    /// Create a processor with additional atom/MIDI sidechain ports.
    ///
    /// Atom ports are connected to an empty atom sequence buffer so plugins
    /// that require a MIDI input port (even if unused) don't crash.
    pub fn with_atom_ports(
        plugin: Lv2Plugin,
        audio_in_ports: &[usize],
        audio_out_ports: &[usize],
        control_ports: &[(usize, f32)],
        atom_ports: &[usize],
    ) -> Self {
        Self::with_extra_ports(plugin, audio_in_ports, audio_out_ports, control_ports, atom_ports, &[])
    }

    /// Create a processor with atom ports and extra (dummy) output ports.
    ///
    /// `extra_out_ports` are connected to a scratch buffer so plugins with
    /// more outputs than we read (e.g., mono-in/stereo-out used as mono)
    /// don't write to unconnected memory.
    pub fn with_extra_ports(
        plugin: Lv2Plugin,
        audio_in_ports: &[usize],
        audio_out_ports: &[usize],
        control_ports: &[(usize, f32)],
        atom_ports: &[usize],
        extra_out_ports: &[usize],
    ) -> Self {
        let mut in_buf = Box::new([0.0f32; MAX_BLOCK_SIZE]);
        let mut out_buf = Box::new([0.0f32; MAX_BLOCK_SIZE]);
        let mut dummy_out_buf = Box::new([0.0f32; MAX_BLOCK_SIZE]);
        let mut control_values: Vec<f32> = control_ports.iter().map(|(_, v)| *v).collect();

        // Create an empty LV2_Atom_Sequence buffer.
        // Layout: [size:u32=8, type:u32=sequence_urid, unit:u32=0, pad:u32=0]
        // We use type=0 which most plugins accept as "no events".
        let mut atom_buf = Box::new([0u8; ATOM_BUF_SIZE]);
        // Set atom.size = 8 (body size: unit + pad)
        atom_buf[0] = 8;
        atom_buf[1] = 0;
        atom_buf[2] = 0;
        atom_buf[3] = 0;
        // atom.type = 0 (plugins typically check for empty sequence by size)
        // body.unit = 0, body.pad = 0 (already zeroed)

        // Connect atom/MIDI sidechain ports
        for &port_idx in atom_ports {
            unsafe {
                plugin.connect_port(
                    port_idx as u32,
                    atom_buf.as_mut_ptr() as *mut c_void,
                );
            }
        }

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

        // Connect extra output ports to dummy buffer (prevents writes to unconnected memory)
        for &port_idx in extra_out_ports {
            unsafe {
                plugin.connect_port(
                    port_idx as u32,
                    dummy_out_buf.as_mut_ptr() as *mut c_void,
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
            _dummy_out_buf: dummy_out_buf,
            control_values,
            _atom_buf: atom_buf,
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
