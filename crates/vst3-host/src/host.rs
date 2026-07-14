//! Low-level VST3 plugin host: loads a bundle, enumerates classes,
//! instantiates IComponent + IAudioProcessor + IEditController and drives the
//! audio processing loop.

use anyhow::{bail, Result};
use std::path::Path;
use std::ptr;
use std::sync::Arc;

use vst3::Steinberg::Vst::{
    AudioBusBuffers, AudioBusBuffers__type0, IAudioProcessor, IAudioProcessorTrait, IComponent,
    IComponentTrait, IEditController, IEditControllerTrait, IParameterChanges, ParameterInfo,
    ProcessData, SymbolicSampleSizes_,
};
use vst3::Steinberg::{kResultOk, IPluginBaseTrait, TBool};
use vst3::{ComPtr, ComWrapper};

use crate::host_application::HostApplication;

use crate::host_utils::char16_array_to_string;
use crate::param_changes::HostParameterChanges;

// ---------------------------------------------------------------------------
// macOS VST3 module lifecycle (bundleEntry / bundleExit)
// ---------------------------------------------------------------------------
//
// The VST3 spec REQUIRES calling the bundle's `bundleEntry(CFBundleRef)` before
// using its factory on macOS, and `bundleExit()` when done. We used to skip it
// (plain `dlopen` + `GetPluginFactory`), which works when nothing else runs,
// but plugins that initialise a GUI/message runtime on entry fail their first
// `createInstance` (result=-1) once the host process already runs an event loop
// — i.e. inside the app but not in a headless test (#251). Calling `bundleEntry`
// on the loading thread (never the main thread) initialises the module cleanly.
#[cfg(target_os = "macos")]
mod cf {
    use std::ffi::c_void;

    pub type CFTypeRef = *mut c_void;
    pub type CFAllocatorRef = *mut c_void;
    pub type CFURLRef = *mut c_void;
    pub type CFBundleRef = *mut c_void;

    pub type CFStringRef = *mut c_void;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        pub fn CFURLCreateFromFileSystemRepresentation(
            allocator: CFAllocatorRef,
            buffer: *const u8,
            buf_len: isize,
            is_directory: u8,
        ) -> CFURLRef;
        pub fn CFBundleCreate(allocator: CFAllocatorRef, url: CFURLRef) -> CFBundleRef;
        pub fn CFBundleGetFunctionPointerForName(
            bundle: CFBundleRef,
            function_name: CFStringRef,
        ) -> *mut c_void;
        pub fn CFStringCreateWithCString(
            allocator: CFAllocatorRef,
            c_str: *const std::os::raw::c_char,
            encoding: u32,
        ) -> CFStringRef;
        pub fn CFRelease(cf: CFTypeRef);
    }

    pub const K_CFSTRING_ENCODING_UTF8: u32 = 0x0800_0100;

    /// Resolve an exported module function (e.g. `bundleEntry`) via the bundle,
    /// which is how VST3 macOS modules expose them (not always via dlsym).
    pub unsafe fn bundle_fn(bundle: CFBundleRef, name: &std::ffi::CStr) -> *mut c_void {
        let cfname = CFStringCreateWithCString(
            std::ptr::null_mut(),
            name.as_ptr(),
            K_CFSTRING_ENCODING_UTF8,
        );
        if cfname.is_null() {
            return std::ptr::null_mut();
        }
        let p = CFBundleGetFunctionPointerForName(bundle, cfname);
        CFRelease(cfname);
        p
    }

    /// `bool bundleEntry(CFBundleRef)` exported by the plugin binary.
    pub type BundleEntryFn = unsafe extern "C" fn(CFBundleRef) -> bool;
    /// `bool bundleExit(void)` exported by the plugin binary.
    pub type BundleExitFn = unsafe extern "C" fn() -> bool;

    /// A retained `CFBundleRef`. `Vst3Plugin` already guarantees single-owner,
    /// non-concurrent use (see its `Send` note); the ref is only touched on
    /// load and drop, so wrapping it Send+Sync is sound.
    pub struct OwnedBundle(pub CFBundleRef);
    unsafe impl Send for OwnedBundle {}
    unsafe impl Sync for OwnedBundle {}
}

/// Create the `CFBundleRef` for the `.vst3` and run its `bundleEntry`, so the
/// module is initialised per the VST3 macOS spec. Returns the (retained) bundle
/// ref, kept alive for the plugin's lifetime and released via `bundleExit` +
/// `CFRelease` on drop. Runs on the caller's (stream/rebuild) thread.
#[cfg(target_os = "macos")]
unsafe fn run_bundle_entry(bundle_path: &Path) -> Result<cf::OwnedBundle> {
    use std::os::unix::ffi::OsStrExt;
    let path_bytes = bundle_path.as_os_str().as_bytes();
    let url = cf::CFURLCreateFromFileSystemRepresentation(
        ptr::null_mut(),
        path_bytes.as_ptr(),
        path_bytes.len() as isize,
        1, // isDirectory: a .vst3 bundle is a directory
    );
    if url.is_null() {
        bail!(
            "CFURLCreateFromFileSystemRepresentation failed for {}",
            bundle_path.display()
        );
    }
    let bundle = cf::CFBundleCreate(ptr::null_mut(), url);
    cf::CFRelease(url);
    if bundle.is_null() {
        bail!("CFBundleCreate failed for {}", bundle_path.display());
    }
    // `bundleEntry` is exposed via the bundle on macOS (not reliably via dlsym).
    // Skipping it is why plugins that spin up a GUI/message runtime fail their
    // first createInstance once the host process runs an event loop (#251).
    let entry_ptr = cf::bundle_fn(bundle, c"bundleEntry");
    if entry_ptr.is_null() {
        log::warn!("VST3 bundleEntry not found on bundle");
    } else {
        let entry: cf::BundleEntryFn = std::mem::transmute(entry_ptr);
        let ok = entry(bundle);
        log::debug!("VST3 bundleEntry called -> {ok}");
        if !ok {
            cf::CFRelease(bundle);
            bail!("VST3 bundleEntry returned false");
        }
    }
    Ok(cf::OwnedBundle(bundle))
}

// ---------------------------------------------------------------------------
// Public data types
// ---------------------------------------------------------------------------

/// Metadata about a plugin parameter.
#[derive(Debug, Clone)]
pub struct Vst3ParamInfo {
    pub id: u32,
    pub title: String,
    pub short_title: String,
    pub units: String,
    pub step_count: i32,
    pub default_normalized: f64,
    /// For a discrete parameter with `step_count >= 2` (a select), one
    /// `(value_percent, label)` per step read from the controller; empty for
    /// continuous knobs and on/off toggles (#780).
    pub enum_options: Vec<(String, String)>,
}

/// A plugin class found in a factory.
#[derive(Debug, Clone)]
pub struct Vst3PluginClass {
    pub uid: [u8; 16],
    pub name: String,
    pub category: String,
}

// ---------------------------------------------------------------------------
// Vst3Plugin
// ---------------------------------------------------------------------------

/// Loaded and initialised VST3 plugin instance ready for audio processing.
///
/// # Safety / Send
///
/// `Vst3Plugin` holds raw COM pointers that are not thread-safe by themselves.
/// It is marked `Send` because the audio thread is the exclusive user of this
/// struct after construction — no concurrent access is ever made. Callers must
/// ensure the plugin is used only on the audio thread.
pub struct Vst3Plugin {
    /// COM state, in `ManuallyDrop` so `Drop` can hand its teardown to the main
    /// thread when the plugin is dropped off it (issue #778): a JUCE plugin's
    /// `terminate()` tears down its native editor, which is main-thread-only on
    /// macOS. `load` (in `host_load.rs`) constructs the inner state.
    inner: std::mem::ManuallyDrop<Vst3Inner>,
}

// `pub` (not re-exported) only so the `Deref` target on the public `Vst3Plugin`
// isn't a private type; the type stays unreachable outside this private module.
pub struct Vst3Inner {
    /// Keep the library alive for the lifetime of the plugin.
    _library: Arc<libloading::Library>,

    /// `IComponent` interface — controls bus routing, activation, state.
    component: ComPtr<IComponent>,

    /// `IAudioProcessor` interface — the actual audio process() call.
    audio_processor: ComPtr<IAudioProcessor>,

    /// `IEditController` interface — parameter read/write.
    /// May be the same object as `component` (via QI) or a separate one.
    controller: ComPtr<IEditController>,

    /// Whether the controller was created as a separate object and needs its
    /// own terminate() call.
    controller_is_separate: bool,

    /// Number of audio input channels (used to set up ProcessData).
    pub num_input_channels: i32,

    /// Number of audio output channels.
    pub num_output_channels: i32,

    /// Internal block size.
    block_size: usize,

    /// The host context passed to `initialize`; kept alive so the plugin can
    /// hold a reference to it for its whole lifetime.
    _host_app: ComWrapper<HostApplication>,

    /// The `CFBundleRef` whose `bundleEntry` initialised this module (macOS).
    /// Kept alive for the plugin's lifetime; released via `bundleExit` +
    /// `CFRelease` on drop.
    #[cfg(target_os = "macos")]
    cf_bundle: cf::OwnedBundle,
}

// Safety: VST3 plugins must only be used on the audio thread. The owner
// (Vst3Processor / StereoVst3Processor) is responsible for that invariant.
// `Send` on the inner state additionally lets a deferred teardown be handed to
// the main thread (issue #778).
unsafe impl Send for Vst3Inner {}
unsafe impl Send for Vst3Plugin {}

impl std::ops::Deref for Vst3Plugin {
    type Target = Vst3Inner;
    fn deref(&self) -> &Vst3Inner {
        &self.inner
    }
}
impl std::ops::DerefMut for Vst3Plugin {
    fn deref_mut(&mut self) -> &mut Vst3Inner {
        &mut self.inner
    }
}

/// `Vst3Plugin::load` — split into its own file to keep this one under the cap.
#[path = "host_load.rs"]
mod load;

impl Vst3Plugin {
    /// Process a stereo (or mono-duplicated) block of audio.
    ///
    /// `input_l`, `input_r` are input channel slices (length = `n_samples`).
    /// `output_l`, `output_r` are output channel slices (length = `n_samples`).
    ///
    /// For mono plugins, pass the same slice for L and R inputs, and read only
    /// L from the outputs.
    ///
    /// # Safety
    ///
    /// All slices must have length >= `n_samples`. The raw pointers in
    /// `ProcessData` are only valid for the duration of this call.
    /// Process audio, optionally applying parameter changes to the DSP.
    ///
    /// `pending_params` — `(param_id, normalized_value)` pairs collected from
    /// the GUI since the last block.  They are delivered via
    /// `ProcessData::inputParameterChanges` so the plugin's DSP reads them
    /// regardless of whether its controller and component share state.
    pub fn process_audio(
        &mut self,
        input_l: &mut [f32],
        input_r: &mut [f32],
        output_l: &mut [f32],
        output_r: &mut [f32],
        n_samples: usize,
        pending_params: &[(u32, f64)],
    ) {
        debug_assert!(input_l.len() >= n_samples);
        debug_assert!(input_r.len() >= n_samples);
        debug_assert!(output_l.len() >= n_samples);
        debug_assert!(output_r.len() >= n_samples);

        let n = n_samples.min(self.block_size) as i32;

        // Build planar channel pointer arrays.
        let mut input_channels: [*mut f32; 2] = [input_l.as_mut_ptr(), input_r.as_mut_ptr()];
        let mut output_channels: [*mut f32; 2] = [output_l.as_mut_ptr(), output_r.as_mut_ptr()];

        let num_in = self.num_input_channels.max(1).min(2) as usize;
        let num_out = self.num_output_channels.max(1).min(2) as usize;

        let mut input_bus = AudioBusBuffers {
            numChannels: num_in as i32,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: input_channels.as_mut_ptr(),
            },
        };
        let mut output_bus = AudioBusBuffers {
            numChannels: num_out as i32,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: output_channels.as_mut_ptr(),
            },
        };

        // Build IParameterChanges COM object for any pending GUI-driven changes.
        // Keeping this alive until after process() ensures the plugin can
        // safely dereference the pointer during the call.
        let param_changes_wrapper: Option<ComWrapper<HostParameterChanges>> =
            if !pending_params.is_empty() {
                Some(ComWrapper::new(HostParameterChanges::new(pending_params)))
            } else {
                None
            };

        let input_param_changes_ptr: *mut IParameterChanges = param_changes_wrapper
            .as_ref()
            .and_then(|w| w.as_com_ref::<IParameterChanges>())
            .map(|r| r.as_ptr())
            .unwrap_or(ptr::null_mut());

        let mut process_data = ProcessData {
            processMode: 0i32, // kRealtime
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            numSamples: n,
            numInputs: 1,
            numOutputs: 1,
            inputs: &mut input_bus,
            outputs: &mut output_bus,
            inputParameterChanges: input_param_changes_ptr,
            outputParameterChanges: ptr::null_mut(),
            inputEvents: ptr::null_mut(),
            outputEvents: ptr::null_mut(),
            processContext: ptr::null_mut(),
        };

        // Safety: process_data is valid for this call. The plugin must not
        // retain any pointers from it after process() returns.
        let res = unsafe { self.audio_processor.process(&mut process_data) };
        if res != kResultOk {
            log::trace!(
                "IAudioProcessor::process returned {} (non-zero, may be normal)",
                res
            );
        }
    }

    /// Set a parameter by its VST3 parameter ID (normalized 0.0..=1.0).
    pub fn set_param(&self, id: u32, normalized: f64) -> Result<()> {
        let res = unsafe { self.controller.setParamNormalized(id, normalized) };
        if res != kResultOk {
            bail!(
                "setParamNormalized({}, {}) returned {}",
                id,
                normalized,
                res
            );
        }
        Ok(())
    }

    /// Get a parameter's current normalized value (0.0..=1.0).
    pub fn get_param(&self, id: u32) -> f64 {
        unsafe { self.controller.getParamNormalized(id) }
    }

    /// Get parameter metadata at the given index.
    pub fn param_info(&self, index: i32) -> Result<Vst3ParamInfo> {
        let mut info: ParameterInfo = unsafe { std::mem::zeroed() };
        let res = unsafe { self.controller.getParameterInfo(index, &mut info) };
        if res != kResultOk {
            bail!("getParameterInfo({}) returned {}", index, res);
        }
        Ok(Vst3ParamInfo {
            id: info.id,
            title: char16_array_to_string(&info.title),
            short_title: char16_array_to_string(&info.shortTitle),
            units: char16_array_to_string(&info.units),
            step_count: info.stepCount,
            default_normalized: info.defaultNormalizedValue,
            enum_options: Vec::new(),
        })
    }

    /// Number of parameters exposed by the plugin.
    pub fn param_count(&self) -> i32 {
        unsafe { self.controller.getParameterCount() }
    }

    /// Access the `IEditController` interface for this plugin (e.g. to create a GUI view).
    pub fn controller(&self) -> &ComPtr<vst3::Steinberg::Vst::IEditController> {
        &self.controller
    }

    /// Access the `IComponent` interface (needed for GUI host setup: IConnectionPoint).
    pub fn component(&self) -> &ComPtr<IComponent> {
        &self.component
    }

    /// Clone the `Arc` wrapping the shared library so the GUI can keep the
    /// dylib alive independently of the audio-thread plugin instance.
    pub fn library_arc(&self) -> Arc<libloading::Library> {
        self._library.clone()
    }

    /// Clone the `IEditController` COM pointer (reference-counted) so the GUI
    /// can reuse the controller from the audio processor without creating a
    /// second plugin instance.
    pub fn controller_clone(&self) -> ComPtr<vst3::Steinberg::Vst::IEditController> {
        self.controller.clone()
    }
}

impl Drop for Vst3Inner {
    fn drop(&mut self) {
        // Serialise teardown with instantiation: concurrent `terminate()` of a
        // JUCE plugin SIGSEGVs just like concurrent `createInstance` (#776).
        let _serialize = crate::main_thread::juce_op_guard();
        // Deactivate processing and component in reverse order.
        unsafe {
            let _ = self.audio_processor.setProcessing(0 as TBool);
            let _ = self.component.setActive(0 as TBool);

            if self.controller_is_separate {
                let _ = self.controller.terminate();
            }
            let _ = self.component.terminate();

            // macOS: balance run_bundle_entry — exit the module (before the
            // library is dlclose'd by the Arc drop) and release the CFBundle.
            #[cfg(target_os = "macos")]
            {
                if !self.cf_bundle.0.is_null() {
                    let exit_ptr = cf::bundle_fn(self.cf_bundle.0, c"bundleExit");
                    if !exit_ptr.is_null() {
                        let exit: cf::BundleExitFn = std::mem::transmute(exit_ptr);
                        let _ = exit();
                    }
                    cf::CFRelease(self.cf_bundle.0);
                }
            }
        }
        log::debug!("VST3: plugin instance dropped");
    }
}

impl Drop for Vst3Plugin {
    fn drop(&mut self) {
        // The actual teardown (`Vst3Inner::drop` -> `terminate()`) tears down the
        // plugin's native editor, which macOS forbids off the main thread. Hand
        // it to the main thread when we are not on it (issue #778); when there is
        // no GUI (CLI/tests/render) it runs inline.
        // Safety: `inner` is taken exactly once — no other code touches it, and
        // `ManuallyDrop` prevents the automatic field drop here.
        let inner = unsafe { std::mem::ManuallyDrop::take(&mut self.inner) };
        crate::main_thread::run_on_main_or_defer(Box::new(move || drop(inner)));
    }
}
