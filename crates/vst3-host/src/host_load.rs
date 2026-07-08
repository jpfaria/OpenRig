//! `Vst3Plugin::load` — bundle load + COM instantiation. Split out of `host.rs`
//! (issue #778) to keep that file under the per-file line cap. Child module of
//! `host`, so it can construct the private `Vst3Inner`.

use anyhow::{bail, Context, Result};
use std::ffi::{c_char, c_void};
use std::path::Path;
use std::ptr;
use std::sync::Arc;

use vst3::Steinberg::Vst::IHostApplication;
use vst3::Steinberg::Vst::{
    BusDirections_, BusInfo, IAudioProcessor, IAudioProcessorTrait, IComponent, IComponentTrait,
    IEditController, IEditControllerTrait, MediaTypes_, ProcessSetup, SpeakerArr,
    SymbolicSampleSizes_,
};
use vst3::Steinberg::{
    kResultOk, FUnknown, IPluginBaseTrait, IPluginFactory, IPluginFactoryTrait, PClassInfo, TUID,
};
use vst3::{ComPtr, Interface};

use crate::host_application::HostApplication;
use crate::host_utils::{bundle_binary_path, tuid_to_bytes};

use super::{Vst3Inner, Vst3Plugin};
#[cfg(target_os = "macos")]
use super::run_bundle_entry;

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
        // Serialise instantiation: concurrent createInstance of a JUCE plugin
        // SIGSEGVs (issue #776). Held for the whole load; the same lock also
        // guards teardown (`Vst3Inner::drop`).
        let _serialize = crate::main_thread::juce_op_guard();

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
        let library = Arc::new(
            unsafe { libloading::Library::new(&binary_path) }.with_context(|| {
                format!("failed to dlopen VST3 binary: {}", binary_path.display())
            })?,
        );

        // 2b. macOS: run the module's `bundleEntry` (VST3 spec) so GUI/message
        // runtimes initialise before `createInstance`. On the loading thread —
        // never the main thread.
        #[cfg(target_os = "macos")]
        let cf_bundle = unsafe { run_bundle_entry(bundle_path)? };

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
        let factory: ComPtr<IPluginFactory> = unsafe { ComPtr::from_raw_unchecked(factory_raw) };

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
        let icomponent_iid_ptr = IComponent::IID.as_ptr() as *const c_char;

        let res =
            unsafe { factory.createInstance(cid_ptr, icomponent_iid_ptr, &mut component_raw) };
        if res != kResultOk || component_raw.is_null() {
            bail!("IPluginFactory::createInstance failed (result={})", res);
        }

        // Safety: createInstance returned a valid IComponent* (non-null, result ok).
        let component: ComPtr<IComponent> =
            unsafe { ComPtr::from_raw_unchecked(component_raw as *mut IComponent) };

        // 6. Initialize IComponent (IPluginBase::initialize) with a real host
        // context (IHostApplication). Passing null makes JUCE-based plugins grab
        // the process NSApplication themselves, which breaks the *second*
        // createInstance in the app (#251). The context is kept alive for the
        // plugin's lifetime in `_host_app`.
        let host_app = HostApplication::new();
        let host_ctx: *mut FUnknown = host_app
            .as_com_ref::<IHostApplication>()
            .map(|r| r.as_ptr() as *mut FUnknown)
            .unwrap_or(ptr::null_mut());
        let res = unsafe { component.initialize(host_ctx) };
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
            1u64 << 19 // kSpeakerM = 0x80000 = bit 19
        } else {
            SpeakerArr::kStereo // 3
        };

        // setBusArrangements: pass the same arrangement for inputs and outputs.
        // Some plugins ignore this call and use their own default.
        let mut in_arr = speaker_arr;
        let mut out_arr = speaker_arr;
        let _res = unsafe { audio_processor.setBusArrangements(&mut in_arr, 1, &mut out_arr, 1) };

        // 9. setupProcessing
        let setup = ProcessSetup {
            processMode: 0i32, // kRealtime
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            maxSamplesPerBlock: block_size as i32,
            sampleRate: sample_rate,
        };
        let res = unsafe { audio_processor.setupProcessing(&setup as *const _ as *mut _) };
        if res != kResultOk {
            log::warn!(
                "IAudioProcessor::setupProcessing returned {} (non-fatal)",
                res
            );
        }

        // 10. Activate audio buses (bus 0 in and out).
        let media_audio = MediaTypes_::kAudio as i32;
        let dir_input = BusDirections_::kInput as i32;
        let dir_output = BusDirections_::kOutput as i32;

        let num_in_buses = unsafe { component.getBusCount(media_audio, dir_input) };
        let num_out_buses = unsafe { component.getBusCount(media_audio, dir_output) };

        for i in 0..num_in_buses {
            let _ = unsafe { component.activateBus(media_audio, dir_input, i, 1u8) };
        }
        for i in 0..num_out_buses {
            let _ = unsafe { component.activateBus(media_audio, dir_output, i, 1u8) };
        }

        // Determine actual channel count from bus info.
        let num_input_channels = if num_in_buses > 0 {
            let mut bus_info: BusInfo = unsafe { std::mem::zeroed() };
            let res = unsafe { component.getBusInfo(media_audio, dir_input, 0, &mut bus_info) };
            if res == kResultOk {
                bus_info.channelCount
            } else {
                n_ch
            }
        } else {
            n_ch
        };
        let num_output_channels = if num_out_buses > 0 {
            let mut bus_info: BusInfo = unsafe { std::mem::zeroed() };
            let res = unsafe { component.getBusInfo(media_audio, dir_output, 0, &mut bus_info) };
            if res == kResultOk {
                bus_info.channelCount
            } else {
                n_ch
            }
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
            log::warn!(
                "IAudioProcessor::setProcessing(true) returned {} (non-fatal)",
                res
            );
        }

        // 13. Get IEditController.
        // First try QueryInterface on the component itself (single-object plugins).
        let (controller, controller_is_separate) = if let Some(ctrl) =
            component.cast::<IEditController>()
        {
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

            let res = unsafe { factory.createInstance(ctrl_cid_ptr, ictrl_iid_ptr, &mut ctrl_raw) };
            if res != kResultOk || ctrl_raw.is_null() {
                bail!("failed to create separate IEditController (result={})", res);
            }

            // Safety: createInstance returned a valid IEditController*.
            let ctrl: ComPtr<IEditController> =
                unsafe { ComPtr::from_raw_unchecked(ctrl_raw as *mut IEditController) };

            let res = unsafe { ctrl.initialize(host_ctx) };
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
                log::warn!(
                    "setParamNormalized({}, {}) returned {}",
                    id,
                    normalized,
                    res
                );
            }
        }

        log::info!(
            "VST3: plugin loaded — {}ch in / {}ch out, block_size={}",
            num_input_channels,
            num_output_channels,
            block_size
        );

        Ok(Vst3Plugin {
            inner: std::mem::ManuallyDrop::new(Vst3Inner {
                _library: library,
                component,
                audio_processor,
                controller,
                controller_is_separate,
                num_input_channels,
                num_output_channels,
                block_size,
                _host_app: host_app,
                #[cfg(target_os = "macos")]
                cf_bundle,
            }),
        })
    }
}
