use anyhow::{bail, Result};
use std::ffi::CString;
use std::os::raw::{c_char, c_void};
unsafe extern "C" {
    fn nam_create(model_path_utf8: *const c_char) -> *mut c_void;
    fn nam_destroy(handle: *mut c_void);
    fn nam_process(handle: *mut c_void, input: *const f32, output: *mut f32, nframes: i32);
}
pub struct NamProcessor {
    handle: *mut c_void,
}
unsafe impl Send for NamProcessor {}
impl Drop for NamProcessor {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { nam_destroy(self.handle) };
            self.handle = std::ptr::null_mut();
        }
    }
}
impl NamProcessor {
    pub fn new(model: &str) -> Result<Self> {
        let model_path = CString::new(model)?;
        let handle = unsafe { nam_create(model_path.as_ptr()) };
        if handle.is_null() {
            bail!("failed to load NAM model '{}'", model);
        }
        Ok(Self { handle })
    }
    pub fn process_sample(&mut self, sample: f32) -> f32 {
        let input = [sample];
        let mut output = [0.0f32];
        unsafe {
            nam_process(self.handle, input.as_ptr(), output.as_mut_ptr(), 1);
        }
        output[0]
    }
}
