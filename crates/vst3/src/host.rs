//! Low-level VST3 plugin host: loads a bundle, enumerates classes,
//! instantiates IComponent + IAudioProcessor + IEditController and drives the
//! audio processing loop.

use anyhow::{bail, Context, Result};
use std::ffi::{c_char, c_void};
use std::path::Path;
use std::ptr;
use std::sync::Arc;

use vst3::Steinberg::Vst::{
    AudioBusBuffers, AudioBusBuffers__type0, BusDirections_, BusInfo, IAudioProcessor,
    IAudioProcessorTrait, IComponent, IComponentTrait, IEditController, IEditControllerTrait,
    IParameterChanges, MediaTypes_, ParameterInfo, ProcessData, ProcessSetup, SpeakerArr,
    SymbolicSampleSizes_,
};
use vst3::Steinberg::{
    IPluginBaseTrait, IPluginFactory, IPluginFactoryTrait, PClassInfo, TBool, TUID,
    kResultOk,
};
use vst3::{ComPtr, ComWrapper, Interface};

use crate::host_utils::{bundle_binary_path, char16_array_to_string, tuid_to_bytes};
use crate::param_changes::HostParameterChanges;

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
}

// Safety: VST3 plugins must only be used on the audio thread. The owner
// (Vst3Processor / StereoVst3Processor) is responsible for that invariant.
unsafe impl Send for Vst3Plugin {}

impl Vst3Plugin {
    /// Load a `.vst3` bundle and instantiate the plugin identified by `uid`.
    ///
    /// - `bundle_path` — path to the `.vst3` bundle directory
    /// - `plugin_uid`  — 16-byte class ID from `IPluginFactory::getClassInfo`
    /// - `sample_rate` — audio sample rate in Hz
    /// - `num_channels`— 1 for mono, 2 for stereo
    /// - `block_size`  — maximum samples per call to `process_audio`
    pub fn load(
        bundle_path: &Path,
        plugin_uid: &[u8; 16],
        sample_rate: f64,
        num_channels: usize,
        block_size: usize,
        initial_params: &[(u32, f64)],
    ) -> Result<Self> {
        // 1. Resolve the binary inside the bundle.
        let binary_path = bundle_binary_path(bundle_path)?;
        log::info!(
            "VST3: loading binary {} for bundle {}",
            binary_path.display(),
            bundle_path.display()
        );

        // 2. dlopen the binary.
        // Safety: libloading loads a shared library. The returned library is
        // kept alive for the entire lifetime of `Vst3Plugin`.
        let library = Arc::new(unsafe { libloading::Library::new(&binary_path) }
            .with_context(|| format!("failed to dlopen VST3 binary: {}", binary_path.display()))?);

        // 3. Get the GetPluginFactory symbol.
        // Safety: the symbol must exist and have the correct signature. This is
        // mandated by the VST3 spec for all conforming plugins.
        let get_factory: libloading::Symbol<unsafe extern "C" fn() -> *mut IPluginFactory> =
            unsafe { library.as_ref().get(b"GetPluginFactory\0") }
                .context("symbol 'GetPluginFactory' not found — not a VST3 plugin")?;

        let factory_raw = unsafe { get_factory() };
        if factory_raw.is_null() {
            bail!("GetPluginFactory returned null");
        }

        // Take ownership of the factory pointer (COM ref count = 1 from the call).
        // Safety: factory_raw is a valid, non-null IPluginFactory* returned by the plugin.
        let factory: ComPtr<IPluginFactory> =
            unsafe { ComPtr::from_raw_unchecked(factory_raw) };

        // 4. Find the class whose UID matches plugin_uid.
        let class_count = unsafe { factory.countClasses() };
        let mut found_tuid: Option<TUID> = None;

        for i in 0..class_count {
            let mut info: PClassInfo = unsafe { std::mem::zeroed() };
            let res = unsafe { factory.getClassInfo(i, &mut info) };
            if res != kResultOk {
                continue;
            }
            let bytes = tuid_to_bytes(&info.cid);
            if &bytes == plugin_uid {
                found_tuid = Some(info.cid);
                break;
            }
        }

        let class_tuid = found_tuid
            .with_context(|| format!("plugin UID {:?} not found in factory", plugin_uid))?;

        // 5. Create IComponent instance.
        let mut component_raw: *mut c_void = ptr::null_mut();
        let cid_ptr = class_tuid.as_ptr() as *const c_char;
        let icomponent_iid_ptr =
            IComponent::IID.as_ptr() as *const c_char;

        let res = unsafe {
            factory.createInstance(cid_ptr, icomponent_iid_ptr, &mut component_raw)
        };
        if res != kResultOk || component_raw.is_null() {
            bail!("IPluginFactory::createInstance failed (result={})", res);
        }

        // Safety: createInstance returned a valid IComponent* (non-null, result ok).
        let component: ComPtr<IComponent> =
            unsafe { ComPtr::from_raw_unchecked(component_raw as *mut IComponent) };

        // 6. Initialize IComponent (IPluginBase::initialize).
        // We pass null as the host context — most plugins accept this.
        let res = unsafe { component.initialize(ptr::null_mut()) };
        if res != kResultOk {
            log::warn!("IComponent::initialize returned {} (non-fatal)", res);
        }

        // 7. Query IAudioProcessor from the same object.
        let audio_processor: ComPtr<IAudioProcessor> = component
            .cast::<IAudioProcessor>()
            .context("plugin does not implement IAudioProcessor")?;

        // 8. Set up bus arrangements.
        let n_ch = num_channels as i32;
        let speaker_arr: u64 = if num_channels == 1 {
            // kMono = L+R both bit 0? No — for mono use kSpeakerM or just 1 channel.
            // VST3 SpeakerArr::kMono is 0x200000 (M speaker), but most plugins
            // accept kStereo (3) for stereo or a single channel. For simplicity
            // we use kStereo regardless and handle mono via buffer zeroing.
            // Actually use the correct mono arrangement.
            1u64 << 19 // kSpeakerM = 0x80000 = bit 19
        } else {
            SpeakerArr::kStereo // 3
        };

        // setBusArrangements: pass the same arrangement for inputs and outputs.
        // Some plugins ignore this call and use their own default.
        let mut in_arr = speaker_arr;
        let mut out_arr = speaker_arr;
        let _res = unsafe {
            audio_processor.setBusArrangements(&mut in_arr, 1, &mut out_arr, 1)
        };

        // 9. setupProcessing
        // SymbolicSampleSizes_ and MediaTypes_ are DefaultEnumType (u32 on macOS/Linux,
        // i32 on Windows) but the struct fields and function parameters use i32.
        // We cast with `as i32` which is safe for these small enum values.
        let setup = ProcessSetup {
            processMode: 0i32, // kRealtime
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            maxSamplesPerBlock: block_size as i32,
            sampleRate: sample_rate,
        };
        let res = unsafe { audio_processor.setupProcessing(&setup as *const _ as *mut _) };
        if res != kResultOk {
            log::warn!("IAudioProcessor::setupProcessing returned {} (non-fatal)", res);
        }

        // 10. Activate audio buses (bus 0 in and out).
        // getBusCount takes (MediaType: i32, BusDirection: i32).
        let media_audio = MediaTypes_::kAudio as i32;
        let dir_input = BusDirections_::kInput as i32;
        let dir_output = BusDirections_::kOutput as i32;

        let num_in_buses = unsafe {
            component.getBusCount(media_audio, dir_input)
        };
        let num_out_buses = unsafe {
            component.getBusCount(media_audio, dir_output)
        };

        for i in 0..num_in_buses {
            let _ = unsafe {
                component.activateBus(media_audio, dir_input, i, 1u8)
            };
        }
        for i in 0..num_out_buses {
            let _ = unsafe {
                component.activateBus(media_audio, dir_output, i, 1u8)
            };
        }

        // Determine actual channel count from bus info.
        let num_input_channels = if num_in_buses > 0 {
            let mut bus_info: BusInfo = unsafe { std::mem::zeroed() };
            let res = unsafe {
                component.getBusInfo(media_audio, dir_input, 0, &mut bus_info)
            };
            if res == kResultOk { bus_info.channelCount } else { n_ch }
        } else {
            n_ch
        };
        let num_output_channels = if num_out_buses > 0 {
            let mut bus_info: BusInfo = unsafe { std::mem::zeroed() };
            let res = unsafe {
                component.getBusInfo(media_audio, dir_output, 0, &mut bus_info)
            };
            if res == kResultOk { bus_info.channelCount } else { n_ch }
        } else {
            n_ch
        };

        // 11. setActive(true) — TBool = u8, so pass 1u8
        let res = unsafe { component.setActive(1u8) };
        if res != kResultOk {
            log::warn!("IComponent::setActive(true) returned {} (non-fatal)", res);
        }

        // 12. setProcessing(true)
        let res = unsafe { audio_processor.setProcessing(1u8) };
        if res != kResultOk {
            log::warn!("IAudioProcessor::setProcessing(true) returned {} (non-fatal)", res);
        }

        // 13. Get IEditController.
        // First try QueryInterface on the component itself (single-object plugins).
        let (controller, controller_is_separate) =
            if let Some(ctrl) = component.cast::<IEditController>() {
                log::debug!("VST3: controller via QueryInterface (same object)");
                (ctrl, false)
            } else {
                // The component reports a separate controller class ID.
                let mut ctrl_class_id: TUID = unsafe { std::mem::zeroed() };
                let res = unsafe { component.getControllerClassId(&mut ctrl_class_id) };
                if res != kResultOk {
                    bail!("plugin has no IEditController and getControllerClassId failed");
                }

                let mut ctrl_raw: *mut c_void = ptr::null_mut();
                let ctrl_cid_ptr = ctrl_class_id.as_ptr() as *const c_char;
                let ictrl_iid_ptr = IEditController::IID.as_ptr() as *const c_char;

                let res = unsafe {
                    factory.createInstance(ctrl_cid_ptr, ictrl_iid_ptr, &mut ctrl_raw)
                };
                if res != kResultOk || ctrl_raw.is_null() {
                    bail!("failed to create separate IEditController (result={})", res);
                }

                // Safety: createInstance returned a valid IEditController*.
                let ctrl: ComPtr<IEditController> =
                    unsafe { ComPtr::from_raw_unchecked(ctrl_raw as *mut IEditController) };

                let res = unsafe { ctrl.initialize(ptr::null_mut()) };
                if res != kResultOk {
                    log::warn!("IEditController::initialize returned {} (non-fatal)", res);
                }

                log::debug!("VST3: controller created as separate object");
                (ctrl, true)
            };

        // 14. Connect component ↔ controller via IConnectionPoint.
        // Required by the VST3 spec so the controller can query component
        // state before createView is called. Many plugins return null from
        // createView if this step is skipped.
        unsafe {
            use vst3::Steinberg::Vst::IConnectionPoint;
            use vst3::Steinberg::Vst::IConnectionPointTrait;
            if let Some(comp_cp) = component.cast::<IConnectionPoint>() {
                if let Some(ctrl_cp) = controller.cast::<IConnectionPoint>() {
                    let _ = comp_cp.connect(ctrl_cp.as_ptr());
                    let _ = ctrl_cp.connect(comp_cp.as_ptr());
                    log::debug!("VST3: IConnectionPoint connected");
                }
            }
        }

        // 15. Apply initial parameters.
        for &(id, normalized) in initial_params {
            let res = unsafe { controller.setParamNormalized(id, normalized) };
            if res != kResultOk {
                log::warn!("setParamNormalized({}, {}) returned {}", id, normalized, res);
            }
        }

        log::info!(
            "VST3: plugin loaded — {}ch in / {}ch out, block_size={}",
            num_input_channels,
            num_output_channels,
            block_size
        );

        Ok(Self {
            _library: library,
            component,
            audio_processor,
            controller,
            controller_is_separate,
            num_input_channels,
            num_output_channels,
            block_size,
        })
    }

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
        let mut input_channels: [*mut f32; 2] =
            [input_l.as_mut_ptr(), input_r.as_mut_ptr()];
        let mut output_channels: [*mut f32; 2] =
            [output_l.as_mut_ptr(), output_r.as_mut_ptr()];

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
            log::trace!("IAudioProcessor::process returned {} (non-zero, may be normal)", res);
        }
    }

    /// Set a parameter by its VST3 parameter ID (normalized 0.0..=1.0).
    pub fn set_param(&self, id: u32, normalized: f64) -> Result<()> {
        let res = unsafe { self.controller.setParamNormalized(id, normalized) };
        if res != kResultOk {
            bail!("setParamNormalized({}, {}) returned {}", id, normalized, res);
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

impl Drop for Vst3Plugin {
    fn drop(&mut self) {
        // Deactivate processing and component in reverse order.
        unsafe {
            let _ = self.audio_processor.setProcessing(0 as TBool);
            let _ = self.component.setActive(0 as TBool);

            if self.controller_is_separate {
                let _ = self.controller.terminate();
            }
            let _ = self.component.terminate();
        }
        log::debug!("VST3: plugin instance dropped");
    }
}
