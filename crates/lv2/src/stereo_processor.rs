//! Responsibility: runs an LV2 plugin over a stereo signal.
use crate::host::Lv2Plugin;
use block_core::StereoProcessor;
use std::ffi::c_void;

/// Maximum block size for LV2 processing (matches `Lv2Processor`).
const MAX_BLOCK_SIZE: usize = 4096;

/// Size of the dummy atom buffer for MIDI/atom sidechain ports.
/// Must meet rsz:minimumSize from plugin TTL (Dragonfly requires 2048).
const ATOM_BUF_SIZE: usize = 4096;

/// Stereo audio processor wrapping a loaded LV2 plugin with two audio
/// outputs and one, two or three audio inputs.
///
/// Unlike `Lv2Processor` (mono), this connects separate L/R output buffers.
/// A 1-in/2-out plugin is fed the mid of the stereo frame on its single
/// input — connecting both of its outputs to one buffer (the mono path)
/// kept only the last one written and printed L == R (#938).
/// A 3-in/2-out plugin (ZamCompX2, ZamGateX2) has L, R and a sidechain
/// input; the sidechain is fed the mid of the same frame, an internal
/// sidechain like the plugin's own with its sidechain switch off (#1105).
pub struct StereoLv2Processor {
    plugin: Lv2Plugin,
    mono_input: bool,
    sidechain_input: bool,
    in_buf_l: Box<[f32; MAX_BLOCK_SIZE]>,
    in_buf_r: Box<[f32; MAX_BLOCK_SIZE]>,
    in_buf_sidechain: Box<[f32; MAX_BLOCK_SIZE]>,
    out_buf_l: Box<[f32; MAX_BLOCK_SIZE]>,
    out_buf_r: Box<[f32; MAX_BLOCK_SIZE]>,
    /// Scratch buffer for output control ports (meters, latency) that
    /// must be connected but are never read (issue #457).
    _dummy_out_buf: Box<[f32; MAX_BLOCK_SIZE]>,
    control_values: Vec<f32>,
    /// Slot the plugin's latency port writes into (#328), boxed so its
    /// address stays put while the port is connected to it.
    latency_out: Box<f32>,
    /// Latency the plugin published on that port, in samples, read once at
    /// build (#328). 0 when the plugin declares no latency port.
    latency: usize,
    _atom_buf: Box<[u8; ATOM_BUF_SIZE]>,
}

impl StereoLv2Processor {
    /// Create a new stereo processor.
    ///
    /// - `audio_in_ports`: `[mono_in]`, `[left_in, right_in]` or
    ///   `[left_in, right_in, sidechain_in]`
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
        Self::with_extra_ports(
            plugin,
            audio_in_ports,
            audio_out_ports,
            control_ports,
            atom_ports,
            &[],
        )
    }

    /// Create a stereo processor with atom ports and extra (dummy)
    /// output ports.
    ///
    /// `extra_out_ports` are output control ports (gain-reduction
    /// meters, latency indicators). LV2 requires every port to be
    /// connected before `run()`; leaving an output control port
    /// unconnected makes the plugin write to null/garbage memory →
    /// SIGSEGV (issue #457). They are connected to a scratch buffer.
    pub fn with_extra_ports(
        plugin: Lv2Plugin,
        audio_in_ports: &[usize],
        audio_out_ports: &[usize],
        control_ports: &[(usize, f32)],
        atom_ports: &[usize],
        extra_out_ports: &[usize],
    ) -> Self {
        assert!(
            matches!(audio_in_ports.len(), 1..=3),
            "stereo requires 1, 2 or 3 audio inputs"
        );
        assert!(
            audio_out_ports.len() == 2,
            "stereo requires 2 audio outputs"
        );
        let mono_input = audio_in_ports.len() == 1;
        let sidechain_input = audio_in_ports.len() == 3;

        let mut in_buf_l = Box::new([0.0f32; MAX_BLOCK_SIZE]);
        let mut in_buf_r = Box::new([0.0f32; MAX_BLOCK_SIZE]);
        let mut in_buf_sidechain = Box::new([0.0f32; MAX_BLOCK_SIZE]);
        let mut out_buf_l = Box::new([0.0f32; MAX_BLOCK_SIZE]);
        let mut out_buf_r = Box::new([0.0f32; MAX_BLOCK_SIZE]);
        let mut dummy_out_buf = Box::new([0.0f32; MAX_BLOCK_SIZE]);
        let mut control_values: Vec<f32> = control_ports.iter().map(|(_, v)| *v).collect();

        let mut atom_buf = Box::new([0u8; ATOM_BUF_SIZE]);
        atom_buf[0] = 8; // atom.size = 8

        for &port_idx in atom_ports {
            unsafe {
                plugin.connect_port(port_idx as u32, atom_buf.as_mut_ptr() as *mut c_void);
            }
        }

        // Connect output control ports to the scratch buffer so the
        // plugin never writes to unconnected memory (issue #457).
        for &port_idx in extra_out_ports {
            unsafe {
                plugin.connect_port(port_idx as u32, dummy_out_buf.as_mut_ptr() as *mut c_void);
            }
        }

        unsafe {
            plugin.connect_port(
                audio_in_ports[0] as u32,
                in_buf_l.as_mut_ptr() as *mut c_void,
            );
            if !mono_input {
                plugin.connect_port(
                    audio_in_ports[1] as u32,
                    in_buf_r.as_mut_ptr() as *mut c_void,
                );
            }
            if sidechain_input {
                plugin.connect_port(
                    audio_in_ports[2] as u32,
                    in_buf_sidechain.as_mut_ptr() as *mut c_void,
                );
            }
            plugin.connect_port(
                audio_out_ports[0] as u32,
                out_buf_l.as_mut_ptr() as *mut c_void,
            );
            plugin.connect_port(
                audio_out_ports[1] as u32,
                out_buf_r.as_mut_ptr() as *mut c_void,
            );
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
            mono_input,
            sidechain_input,
            in_buf_l,
            in_buf_r,
            in_buf_sidechain,
            out_buf_l,
            out_buf_r,
            _dummy_out_buf: dummy_out_buf,
            control_values,
            latency_out: Box::new(0.0),
            latency: 0,
            _atom_buf: atom_buf,
        }
    }

    pub fn set_control(&mut self, control_index: usize, value: f32) {
        if control_index < self.control_values.len() {
            self.control_values[control_index] = value;
        }
    }

    /// Point the plugin's latency port at this processor's own slot, let the
    /// plugin publish its latency on one silent frame, and keep the value
    /// (#328). Build time only — never on the audio thread.
    pub fn with_latency_port(mut self, port_idx: usize) -> Self {
        unsafe {
            self.plugin.connect_port(
                port_idx as u32,
                &mut *self.latency_out as *mut f32 as *mut c_void,
            );
        }
        self.load_input(0, [0.0, 0.0]);
        self.plugin.run(1);
        self.latency = crate::processor::latency_from_port(*self.latency_out);
        self
    }

    fn load_input(&mut self, i: usize, frame: [f32; 2]) {
        if self.mono_input {
            self.in_buf_l[i] = 0.5 * (frame[0] + frame[1]);
        } else {
            self.in_buf_l[i] = frame[0];
            self.in_buf_r[i] = frame[1];
            if self.sidechain_input {
                self.in_buf_sidechain[i] = 0.5 * (frame[0] + frame[1]);
            }
        }
    }
}

impl StereoProcessor for StereoLv2Processor {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.load_input(0, input);
        self.plugin.run(1);
        [self.out_buf_l[0], self.out_buf_r[0]]
    }

    fn process_block(&mut self, buffer: &mut [[f32; 2]]) {
        let len = buffer.len().min(MAX_BLOCK_SIZE);

        for (i, frame) in buffer[..len].iter().enumerate() {
            self.load_input(i, *frame);
        }

        self.plugin.run(len as u32);

        for (i, frame) in buffer[..len].iter_mut().enumerate() {
            frame[0] = self.out_buf_l[i];
            frame[1] = self.out_buf_r[i];
        }
    }

    fn latency_samples(&self) -> usize {
        self.latency
    }
}
