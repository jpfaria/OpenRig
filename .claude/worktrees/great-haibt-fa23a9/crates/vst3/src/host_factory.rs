//! Factory-level VST3 introspection: enumerate plugin classes and read
//! vendor metadata without instantiating the plugin. Lifted out of
//! `host.rs` so the main module stays under the size cap.

use anyhow::{bail, Context, Result};
use std::ffi::CStr;
use std::path::Path;

use vst3::ComPtr;
use vst3::Steinberg::{kResultOk, IPluginFactory, IPluginFactoryTrait, PClassInfo};

use crate::host::{Vst3Plugin, Vst3PluginClass};
use crate::host_utils::{bundle_binary_path, cstr_array_to_string, tuid_to_bytes};

impl Vst3Plugin {
    /// Open the bundle, enumerate classes via `IPluginFactory`, and return
    /// the loaded library together with the class list.
    ///
    /// The library MUST outlive any `ComPtr` derived from the factory; the
    /// caller takes ownership and is responsible for dropping it after the
    /// factory pointers are no longer in use.
    pub fn enumerate_classes(
        bundle_path: &Path,
    ) -> Result<(libloading::Library, Vec<Vst3PluginClass>)> {
        let binary_path = bundle_binary_path(bundle_path)?;
        let library = unsafe { libloading::Library::new(&binary_path) }
            .with_context(|| format!("failed to dlopen VST3 binary: {}", binary_path.display()))?;

        let get_factory: libloading::Symbol<unsafe extern "C" fn() -> *mut IPluginFactory> =
            unsafe { library.get(b"GetPluginFactory\0") }
                .context("symbol 'GetPluginFactory' not found")?;

        let factory_raw = unsafe { get_factory() };
        if factory_raw.is_null() {
            bail!("GetPluginFactory returned null");
        }
        let factory: ComPtr<IPluginFactory> = unsafe { ComPtr::from_raw_unchecked(factory_raw) };

        let count = unsafe { factory.countClasses() };
        let mut classes = Vec::new();
        for i in 0..count {
            let mut info: PClassInfo = unsafe { std::mem::zeroed() };
            if unsafe { factory.getClassInfo(i, &mut info) } != kResultOk {
                continue;
            }
            classes.push(Vst3PluginClass {
                uid: tuid_to_bytes(&info.cid),
                name: cstr_array_to_string(&info.name),
                category: cstr_array_to_string(&info.category),
            });
        }
        Ok((library, classes))
    }

    /// Read vendor string from the factory info (best effort, empty on failure).
    pub fn factory_vendor(bundle_path: &Path) -> String {
        let binary_path = match bundle_binary_path(bundle_path) {
            Ok(p) => p,
            Err(_) => return String::new(),
        };
        let library = match unsafe { libloading::Library::new(&binary_path) } {
            Ok(l) => l,
            Err(_) => return String::new(),
        };
        let get_factory_res: Result<
            libloading::Symbol<unsafe extern "C" fn() -> *mut IPluginFactory>,
            _,
        > = unsafe { library.get(b"GetPluginFactory\0") };

        let get_factory = match get_factory_res {
            Ok(f) => f,
            Err(_) => return String::new(),
        };
        let factory_raw = unsafe { get_factory() };
        if factory_raw.is_null() {
            return String::new();
        }
        let factory: ComPtr<IPluginFactory> = unsafe { ComPtr::from_raw_unchecked(factory_raw) };

        let mut finfo: vst3::Steinberg::PFactoryInfo = unsafe { std::mem::zeroed() };
        if unsafe { factory.getFactoryInfo(&mut finfo) } != kResultOk {
            return String::new();
        }
        // Safety: vendor is a c_char array from a C struct — we read until NUL.
        let raw = unsafe { CStr::from_ptr(finfo.vendor.as_ptr()) };
        raw.to_string_lossy().into_owned()
    }
}

/// Enumerate all classes in a VST3 bundle using only the factory (no full load).
/// Useful for quick scanning without full plugin initialisation.
#[allow(dead_code)]
pub fn list_bundle_classes(bundle_path: &Path) -> Result<Vec<Vst3PluginClass>> {
    let (_lib, classes) = Vst3Plugin::enumerate_classes(bundle_path)?;
    Ok(classes)
}
