use anyhow::{bail, Result};
use stage_core::MonoProcessor;
use std::ffi::CString;
use std::os::raw::{c_char, c_void};

pub const DEFAULT_NAM_MODEL: &str = "neural_amp_modeler";

#[repr(C)]
struct NamPluginConfig {
    model_path_utf8: *const c_char,
    ir_path_utf8: *const c_char,
    input_db: f32,
    output_db: f32,
    noise_gate_threshold_db: f32,
    bass: f32,
    middle: f32,
    treble: f32,
    noise_gate_enabled: u8,
    eq_enabled: u8,
    ir_enabled: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct NamPluginParams {
    pub input_level_db: f32,
    pub output_level_db: f32,
    pub noise_gate_threshold_db: f32,
    pub noise_gate_enabled: bool,
    pub eq_enabled: bool,
    pub ir_enabled: bool,
    pub bass: f32,
    pub middle: f32,
    pub treble: f32,
}

unsafe extern "C" {
    fn nam_create(config: *const NamPluginConfig) -> *mut c_void;
    fn nam_destroy(handle: *mut c_void);
    fn nam_process(handle: *mut c_void, input: *const f32, output: *mut f32, nframes: i32);
}
pub struct NamProcessor {
    handle: *mut c_void,
}
unsafe impl Send for NamProcessor {}
unsafe impl Sync for NamProcessor {}
impl Drop for NamProcessor {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { nam_destroy(self.handle) };
            self.handle = std::ptr::null_mut();
        }
    }
}
impl NamProcessor {
    pub fn new(model_path: &str, ir_path: Option<&str>, params: NamPluginParams) -> Result<Self> {
        let model_path = CString::new(model_path)?;
        let ir_path = ir_path.map(CString::new).transpose()?;
        let config = NamPluginConfig {
            model_path_utf8: model_path.as_ptr(),
            ir_path_utf8: ir_path.as_ref().map_or(std::ptr::null(), |value| value.as_ptr()),
            input_db: params.input_level_db,
            output_db: params.output_level_db,
            noise_gate_threshold_db: params.noise_gate_threshold_db,
            bass: params.bass,
            middle: params.middle,
            treble: params.treble,
            noise_gate_enabled: params.noise_gate_enabled as u8,
            eq_enabled: params.eq_enabled as u8,
            ir_enabled: params.ir_enabled as u8,
        };
        let handle = unsafe { nam_create(&config) };
        if handle.is_null() {
            bail!("failed to load NAM model '{}'", model_path.to_string_lossy());
        }
        Ok(Self { handle })
    }
}

impl MonoProcessor for NamProcessor {
    fn process_sample(&mut self, sample: f32) -> f32 {
        let input = [sample];
        let mut output = [0.0f32];
        unsafe {
            nam_process(self.handle, input.as_ptr(), output.as_mut_ptr(), 1);
        }
        output[0]
    }
}
