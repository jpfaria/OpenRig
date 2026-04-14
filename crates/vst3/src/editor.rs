//! VST3 editor window: opens the plugin's native GUI in a separate OS window.
//!
//! Reuses the `IEditController` from the audio processor (via `Vst3GuiContext`)
//! instead of loading a second plugin instance. This avoids failures with
//! plugins like ValhallaSupermassive that reject multiple instances.
//!
//! Must be called on the main/UI thread (macOS AppKit requirement).

use anyhow::{bail, Result};
use std::path::Path;
use std::sync::Arc;
use vst3::Steinberg::Vst::{IEditControllerTrait, ViewType};
use vst3::Steinberg::IPlugViewTrait;
use vst3::ComPtr;

use crate::component_handler::ComponentHandler;
use crate::host::Vst3Plugin;
use crate::param_registry::Vst3GuiContext;

/// Handle to an open VST3 editor window.
///
/// Closing/dropping this handle calls `IPlugView::removed()` and releases
/// all resources. On macOS it also releases the NSWindow.
pub struct Vst3EditorHandle {
    view: ComPtr<vst3::Steinberg::IPlugView>,
    /// Keeps the plugin dylib alive while the editor is open.
    _library: Arc<libloading::Library>,
    /// Keeps the controller alive so `view.removed()` can call back into it.
    _controller: ComPtr<vst3::Steinberg::Vst::IEditController>,
    /// Keep the ComponentHandler alive for as long as the editor is open.
    _component_handler: Option<vst3::ComWrapper<ComponentHandler>>,
    /// In standalone mode (no engine running) the entire plugin instance must
    /// stay alive because the component must remain active for the view to work.
    _standalone_plugin: Option<Box<Vst3Plugin>>,
    /// Standalone window (None when embedded in parent window).
    #[cfg(target_os = "macos")]
    _ns_window: Option<macos::OwnedNsWindow>,
    /// Child NSView used for embedded mode (None when using standalone window).
    #[cfg(target_os = "macos")]
    _embedded_view: Option<macos::OwnedNsView>,
    /// Whether _ns_window is a borderless child window (not a standalone titled window).
    #[cfg(target_os = "macos")]
    is_child_window: bool,
    /// Parent NSWindow pointer (needed for coordinate conversion in child window mode).
    #[cfg(target_os = "macos")]
    parent_ns_window: *mut std::ffi::c_void,
    /// Editor dimensions in logical pixels.
    editor_width: f64,
    editor_height: f64,
}

// SAFETY: Vst3EditorHandle is only used on the main/UI thread. The raw
// pointer `parent_ns_window` is kept for coordinate conversion and is never
// dereferenced from another thread.
unsafe impl Send for Vst3EditorHandle {}

impl block_core::PluginEditorHandle for Vst3EditorHandle {
    fn reposition_embedded(&self, x: f64, y: f64) {
        #[cfg(target_os = "macos")]
        {
            if self.is_child_window {
                // Child window mode: convert window-relative coords to screen coords.
                if let Some(ref win) = self._ns_window {
                    if !self.parent_ns_window.is_null() {
                        let (sx, sy) = macos::window_to_screen_coords(
                            self.parent_ns_window, x, y, self.editor_height,
                        );
                        win.set_frame_origin(sx, sy);
                    }
                }
            } else if let Some(ref view) = self._embedded_view {
                view.reposition(x, y);
            }
        }
        #[cfg(not(target_os = "macos"))]
        { let _ = (x, y); }
    }

    fn hide_embedded(&self) {
        #[cfg(target_os = "macos")]
        {
            if self.is_child_window {
                if let Some(ref win) = self._ns_window {
                    win.set_visible(false);
                }
            } else if let Some(ref view) = self._embedded_view {
                view.set_hidden(true);
            }
        }
    }

    fn show_embedded(&self) {
        #[cfg(target_os = "macos")]
        {
            if self.is_child_window {
                if let Some(ref win) = self._ns_window {
                    win.set_visible(true);
                }
            } else if let Some(ref view) = self._embedded_view {
                view.set_hidden(false);
            }
        }
    }

    fn editor_size(&self) -> (f64, f64) {
        (self.editor_width, self.editor_height)
    }
}

impl Drop for Vst3EditorHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = self.view.removed();
        }
    }
}

/// Open the native editor window for the VST3 plugin, reusing the existing
/// `IEditController` from the audio processor.
///
/// `plugin_name` is used only for the window title.
/// `gui_context` carries the shared controller, library handle, and param channel.
/// `parent_window` is an optional platform-specific parent window handle.
/// On macOS this is an `NSWindow*`; the editor window is added as a child
/// so it floats above and moves with the parent. Pass null for a standalone
/// floating window.
///
/// Returns a `Vst3EditorHandle` that keeps the window alive. Drop it to close.
pub fn open_vst3_editor_window(
    plugin_name: &str,
    gui_context: Vst3GuiContext,
) -> Result<Vst3EditorHandle> {
    let controller = gui_context.controller;

    // Get IPlugView from the controller.
    let view_ptr = unsafe { controller.createView(ViewType::kEditor) };
    if view_ptr.is_null() {
        bail!("plugin '{}' returned null IPlugView (no GUI)", plugin_name);
    }

    #[cfg(target_os = "macos")]
    {
        use vst3::Steinberg::{kPlatformTypeNSView, kResultOk, ViewRect};

        let library = gui_context.library;
        let param_channel = gui_context.param_channel;

        // Register the component handler so parameter changes from the native GUI
        // reach the audio processor via the param channel.
        let component_handler = {
            let wrapper = ComponentHandler::new(param_channel).into_com_ptr();
            unsafe {
                use vst3::Steinberg::Vst::IComponentHandler;
                if let Some(com_ref) = wrapper.as_com_ref::<IComponentHandler>() {
                    let _ = controller.setComponentHandler(com_ref.as_ptr());
                    log::debug!("VST3 editor: IComponentHandler registered");
                }
            }
            Some(wrapper)
        };

        let view: ComPtr<vst3::Steinberg::IPlugView> =
            unsafe { ComPtr::from_raw_unchecked(view_ptr) };

        let res = unsafe { view.isPlatformTypeSupported(kPlatformTypeNSView) };
        if res != kResultOk {
            bail!("plugin '{}' does not support NSView GUI (result={})", plugin_name, res);
        }

        let mut rect = ViewRect { left: 0, top: 0, right: 800, bottom: 600 };
        unsafe { view.getSize(&mut rect) };
        let width = (rect.right - rect.left).max(200) as f64;
        let height = (rect.bottom - rect.top).max(100) as f64;

        let ns_window = macos::create_editor_window(plugin_name, width, height)?;
        let ns_view = ns_window.content_view();

        let res = unsafe { view.attached(ns_view, kPlatformTypeNSView) };
        if res != kResultOk {
            bail!("IPlugView::attached failed (result={})", res);
        }

        ns_window.show(plugin_name);

        return Ok(Vst3EditorHandle {
            view,
            _library: library,
            _controller: controller,
            _component_handler: component_handler,
            _standalone_plugin: None,
            _ns_window: Some(ns_window),
            _embedded_view: None,
            is_child_window: false,
            parent_ns_window: std::ptr::null_mut(),
            editor_width: width,
            editor_height: height,
        });
    }

    #[cfg(not(target_os = "macos"))]
    bail!("VST3 editor window not yet supported on this platform")
}

/// Open the native editor window as a child of a parent window.
///
/// The editor window is registered via `addChildWindow:ordered:` so it
/// always floats above the parent and moves with it. The title bar is kept
/// for the user to see the plugin name.
///
/// `parent_ns_window` is an `NSWindow*` obtained from `raw-window-handle`.
pub fn open_vst3_editor_window_parented(
    plugin_name: &str,
    gui_context: Vst3GuiContext,
    parent_ns_window: *mut std::ffi::c_void,
) -> Result<Vst3EditorHandle> {
    let controller = gui_context.controller;

    let view_ptr = unsafe { controller.createView(ViewType::kEditor) };
    if view_ptr.is_null() {
        bail!("plugin '{}' returned null IPlugView (no GUI)", plugin_name);
    }

    #[cfg(target_os = "macos")]
    {
        use vst3::Steinberg::{kPlatformTypeNSView, kResultOk, ViewRect};

        let library = gui_context.library;
        let param_channel = gui_context.param_channel;

        let component_handler = {
            let wrapper = ComponentHandler::new(param_channel).into_com_ptr();
            unsafe {
                use vst3::Steinberg::Vst::IComponentHandler;
                if let Some(com_ref) = wrapper.as_com_ref::<IComponentHandler>() {
                    let _ = controller.setComponentHandler(com_ref.as_ptr());
                }
            }
            Some(wrapper)
        };

        let view: ComPtr<vst3::Steinberg::IPlugView> =
            unsafe { ComPtr::from_raw_unchecked(view_ptr) };

        let res = unsafe { view.isPlatformTypeSupported(kPlatformTypeNSView) };
        if res != kResultOk {
            bail!("plugin '{}' does not support NSView GUI (result={})", plugin_name, res);
        }

        let mut rect = ViewRect { left: 0, top: 0, right: 800, bottom: 600 };
        unsafe { view.getSize(&mut rect) };
        let width = (rect.right - rect.left).max(200) as f64;
        let height = (rect.bottom - rect.top).max(100) as f64;

        let ns_window = macos::create_editor_window(plugin_name, width, height)?;
        let ns_view = ns_window.content_view();

        let res = unsafe { view.attached(ns_view, kPlatformTypeNSView) };
        if res != kResultOk {
            bail!("IPlugView::attached failed (result={})", res);
        }

        // Register as child of the parent window so it moves with it.
        if !parent_ns_window.is_null() {
            ns_window.add_as_child_of(parent_ns_window);
        }

        ns_window.show(plugin_name);

        return Ok(Vst3EditorHandle {
            view,
            _library: library,
            _controller: controller,
            _component_handler: component_handler,
            _standalone_plugin: None,
            _ns_window: Some(ns_window),
            _embedded_view: None,
            is_child_window: false,
            parent_ns_window: std::ptr::null_mut(),
            editor_width: width,
            editor_height: height,
        });
    }

    #[cfg(not(target_os = "macos"))]
    bail!("VST3 editor window not yet supported on this platform")
}

/// Open the editor by loading a **fresh** plugin instance.
///
/// Used as a fallback when no `Vst3GuiContext` exists in the registry (i.e.
/// the audio engine has not yet built the chain). Plugins that reject multiple
/// simultaneous instances will fail here if an audio instance is already
/// running, but single-instance plugins (Cloud Seed, Cocoa Delay, …) work
/// fine before the engine starts.
pub fn open_vst3_editor_window_standalone(
    bundle_path: &Path,
    uid: &[u8; 16],
    plugin_name: &str,
    sample_rate: f64,
) -> Result<Vst3EditorHandle> {
    let plugin = Vst3Plugin::load(bundle_path, uid, sample_rate, 2, 512, &[])?;

    // Connect component ↔ controller so createView works (many plugins require it).
    unsafe {
        use vst3::Steinberg::Vst::IConnectionPoint;
        use vst3::Steinberg::Vst::IConnectionPointTrait;
        if let Some(comp_cp) = plugin.component().cast::<IConnectionPoint>() {
            if let Some(ctrl_cp) = plugin.controller().cast::<IConnectionPoint>() {
                let _ = comp_cp.connect(ctrl_cp.as_ptr());
                let _ = ctrl_cp.connect(comp_cp.as_ptr());
                log::debug!("VST3 standalone editor: IConnectionPoint connected");
            }
        }
    }

    let controller = plugin.controller_clone();

    let view_ptr = unsafe { controller.createView(ViewType::kEditor) };
    if view_ptr.is_null() {
        bail!("plugin '{}' returned null IPlugView (no GUI)", plugin_name);
    }

    #[cfg(target_os = "macos")]
    {
        use vst3::Steinberg::{kPlatformTypeNSView, kResultOk, ViewRect};

        let library = plugin.library_arc();

        // No param channel — engine not running.
        let component_handler = {
            let wrapper = ComponentHandler::new(crate::param_channel::vst3_param_channel()).into_com_ptr();
            unsafe {
                use vst3::Steinberg::Vst::IComponentHandler;
                if let Some(com_ref) = wrapper.as_com_ref::<IComponentHandler>() {
                    let _ = controller.setComponentHandler(com_ref.as_ptr());
                }
            }
            Some(wrapper)
        };

        let view: ComPtr<vst3::Steinberg::IPlugView> =
            unsafe { ComPtr::from_raw_unchecked(view_ptr) };

        let res = unsafe { view.isPlatformTypeSupported(kPlatformTypeNSView) };
        if res != kResultOk {
            bail!("plugin '{}' does not support NSView GUI (result={})", plugin_name, res);
        }

        let mut rect = ViewRect { left: 0, top: 0, right: 800, bottom: 600 };
        unsafe { view.getSize(&mut rect) };
        let width = (rect.right - rect.left).max(200) as f64;
        let height = (rect.bottom - rect.top).max(100) as f64;

        let ns_window = macos::create_editor_window(plugin_name, width, height)?;
        let ns_view = ns_window.content_view();

        let res = unsafe { view.attached(ns_view, kPlatformTypeNSView) };
        if res != kResultOk {
            bail!("IPlugView::attached failed (result={})", res);
        }

        ns_window.show(plugin_name);

        return Ok(Vst3EditorHandle {
            view,
            _library: library,
            _controller: controller,
            _component_handler: component_handler,
            _standalone_plugin: Some(Box::new(plugin)),
            _ns_window: Some(ns_window),
            _embedded_view: None,
            is_child_window: false,
            parent_ns_window: std::ptr::null_mut(),
            editor_width: width,
            editor_height: height,
        });
    }

    #[cfg(not(target_os = "macos"))]
    bail!("VST3 editor window not yet supported on this platform")
}

/// Open the VST3 editor embedded inside a parent view (no new OS window).
///
/// `parent_ns_view` must be a valid `NSView*` pointer (e.g. from Slint's raw
/// window handle). The plugin view is attached to a child NSView positioned
/// at `(x, y)` within the parent, using the parent's coordinate system
/// (Slint-style top-left origin is automatically converted if needed).
///
/// Returns `(handle, editor_width, editor_height)`.
///
/// Must be called on the main/UI thread.
pub fn open_vst3_editor_embedded(
    plugin_name: &str,
    gui_context: Vst3GuiContext,
    parent_ns_view: *mut std::ffi::c_void,
    x: f64,
    y: f64,
) -> Result<(Vst3EditorHandle, f64, f64)> {
    let controller = gui_context.controller;

    let view_ptr = unsafe { controller.createView(vst3::Steinberg::Vst::ViewType::kEditor) };
    if view_ptr.is_null() {
        bail!("plugin '{}' returned null IPlugView (no GUI)", plugin_name);
    }

    #[cfg(target_os = "macos")]
    {
        use vst3::Steinberg::{kPlatformTypeNSView, kResultOk, ViewRect};

        let library = gui_context.library;
        let param_channel = gui_context.param_channel;

        let component_handler = {
            let wrapper = ComponentHandler::new(param_channel).into_com_ptr();
            unsafe {
                use vst3::Steinberg::Vst::IComponentHandler;
                if let Some(com_ref) = wrapper.as_com_ref::<IComponentHandler>() {
                    let _ = controller.setComponentHandler(com_ref.as_ptr());
                }
            }
            Some(wrapper)
        };

        let view: ComPtr<vst3::Steinberg::IPlugView> =
            unsafe { ComPtr::from_raw_unchecked(view_ptr) };

        let res = unsafe { view.isPlatformTypeSupported(kPlatformTypeNSView) };
        if res != kResultOk {
            bail!("plugin '{}' does not support NSView GUI (result={})", plugin_name, res);
        }

        let mut rect = ViewRect { left: 0, top: 0, right: 800, bottom: 600 };
        unsafe { view.getSize(&mut rect) };
        let width = (rect.right - rect.left).max(200) as f64;
        let height = (rect.bottom - rect.top).max(100) as f64;

        let child_view = macos::create_child_nsview(parent_ns_view, x, y, width, height)?;
        let ns_view_ptr = child_view.ptr();

        let res = unsafe { view.attached(ns_view_ptr, kPlatformTypeNSView) };
        if res != kResultOk {
            bail!("IPlugView::attached failed (result={})", res);
        }

        return Ok((Vst3EditorHandle {
            view,
            _library: library,
            _controller: controller,
            _component_handler: component_handler,
            _standalone_plugin: None,
            _ns_window: None,
            _embedded_view: Some(child_view),
            is_child_window: false,
            parent_ns_window: std::ptr::null_mut(),
            editor_width: width,
            editor_height: height,
        }, width, height));
    }

    #[cfg(not(target_os = "macos"))]
    bail!("VST3 embedded editor not yet supported on this platform")
}

/// Open the VST3 editor embedded (standalone mode — no engine context).
///
/// Returns `(handle, editor_width, editor_height)`.
pub fn open_vst3_editor_embedded_standalone(
    bundle_path: &Path,
    uid: &[u8; 16],
    plugin_name: &str,
    sample_rate: f64,
    parent_ns_view: *mut std::ffi::c_void,
    x: f64,
    y: f64,
) -> Result<(Vst3EditorHandle, f64, f64)> {
    let plugin = Vst3Plugin::load(bundle_path, uid, sample_rate, 2, 512, &[])?;

    unsafe {
        use vst3::Steinberg::Vst::IConnectionPoint;
        use vst3::Steinberg::Vst::IConnectionPointTrait;
        if let Some(comp_cp) = plugin.component().cast::<IConnectionPoint>() {
            if let Some(ctrl_cp) = plugin.controller().cast::<IConnectionPoint>() {
                let _ = comp_cp.connect(ctrl_cp.as_ptr());
                let _ = ctrl_cp.connect(comp_cp.as_ptr());
            }
        }
    }

    let controller = plugin.controller_clone();

    let view_ptr = unsafe { controller.createView(vst3::Steinberg::Vst::ViewType::kEditor) };
    if view_ptr.is_null() {
        bail!("plugin '{}' returned null IPlugView (no GUI)", plugin_name);
    }

    #[cfg(target_os = "macos")]
    {
        use vst3::Steinberg::{kPlatformTypeNSView, kResultOk, ViewRect};

        let library = plugin.library_arc();

        let component_handler = {
            let wrapper = ComponentHandler::new(crate::param_channel::vst3_param_channel()).into_com_ptr();
            unsafe {
                use vst3::Steinberg::Vst::IComponentHandler;
                if let Some(com_ref) = wrapper.as_com_ref::<IComponentHandler>() {
                    let _ = controller.setComponentHandler(com_ref.as_ptr());
                }
            }
            Some(wrapper)
        };

        let view: ComPtr<vst3::Steinberg::IPlugView> =
            unsafe { ComPtr::from_raw_unchecked(view_ptr) };

        let res = unsafe { view.isPlatformTypeSupported(kPlatformTypeNSView) };
        if res != kResultOk {
            bail!("plugin '{}' does not support NSView GUI (result={})", plugin_name, res);
        }

        let mut rect = ViewRect { left: 0, top: 0, right: 800, bottom: 600 };
        unsafe { view.getSize(&mut rect) };
        let width = (rect.right - rect.left).max(200) as f64;
        let height = (rect.bottom - rect.top).max(100) as f64;

        let child_view = macos::create_child_nsview(parent_ns_view, x, y, width, height)?;
        let ns_view_ptr = child_view.ptr();

        let res = unsafe { view.attached(ns_view_ptr, kPlatformTypeNSView) };
        if res != kResultOk {
            bail!("IPlugView::attached failed (result={})", res);
        }

        return Ok((Vst3EditorHandle {
            view,
            _library: library,
            _controller: controller,
            _component_handler: component_handler,
            _standalone_plugin: Some(Box::new(plugin)),
            _ns_window: None,
            _embedded_view: Some(child_view),
            is_child_window: false,
            parent_ns_window: std::ptr::null_mut(),
            editor_width: width,
            editor_height: height,
        }, width, height));
    }

    #[cfg(not(target_os = "macos"))]
    bail!("VST3 embedded editor not yet supported on this platform")
}

/// Open the VST3 editor in a borderless child window of the main window.
///
/// This is the preferred approach for desktop (non-fullscreen) mode: the Slint
/// popup header stays in the main window (and remains clickable), while the
/// plugin's native GUI renders in a borderless child NSWindow positioned right
/// below the header.
///
/// `parent_ns_window` is the main Slint window's NSWindow pointer.
/// `x`, `y` are Slint logical coordinates (top-left origin, relative to the
/// main window) where the plugin content should appear.
///
/// Returns `(handle, editor_width, editor_height)`.
pub fn open_vst3_editor_child_window(
    plugin_name: &str,
    gui_context: Vst3GuiContext,
    parent_ns_window: *mut std::ffi::c_void,
    x: f64,
    y: f64,
) -> Result<(Vst3EditorHandle, f64, f64)> {
    let controller = gui_context.controller;

    let view_ptr = unsafe { controller.createView(vst3::Steinberg::Vst::ViewType::kEditor) };
    if view_ptr.is_null() {
        bail!("plugin '{}' returned null IPlugView (no GUI)", plugin_name);
    }

    #[cfg(target_os = "macos")]
    {
        use vst3::Steinberg::{kPlatformTypeNSView, kResultOk, ViewRect};

        let library = gui_context.library;
        let param_channel = gui_context.param_channel;

        let component_handler = {
            let wrapper = ComponentHandler::new(param_channel).into_com_ptr();
            unsafe {
                use vst3::Steinberg::Vst::IComponentHandler;
                if let Some(com_ref) = wrapper.as_com_ref::<IComponentHandler>() {
                    let _ = controller.setComponentHandler(com_ref.as_ptr());
                }
            }
            Some(wrapper)
        };

        let view: ComPtr<vst3::Steinberg::IPlugView> =
            unsafe { ComPtr::from_raw_unchecked(view_ptr) };

        let res = unsafe { view.isPlatformTypeSupported(kPlatformTypeNSView) };
        if res != kResultOk {
            bail!("plugin '{}' does not support NSView GUI (result={})", plugin_name, res);
        }

        let mut rect = ViewRect { left: 0, top: 0, right: 800, bottom: 600 };
        unsafe { view.getSize(&mut rect) };
        let width = (rect.right - rect.left).max(200) as f64;
        let height = (rect.bottom - rect.top).max(100) as f64;

        let child_win = macos::create_borderless_child_window(parent_ns_window, width, height)?;

        // Position the child window at the correct screen coordinates.
        let (sx, sy) = macos::window_to_screen_coords(parent_ns_window, x, y, height);
        child_win.set_frame_origin(sx, sy);

        let content_view = child_win.content_view();
        let res = unsafe { view.attached(content_view, kPlatformTypeNSView) };
        if res != kResultOk {
            bail!("IPlugView::attached failed (result={})", res);
        }

        child_win.set_visible(true);

        return Ok((Vst3EditorHandle {
            view,
            _library: library,
            _controller: controller,
            _component_handler: component_handler,
            _standalone_plugin: None,
            _ns_window: Some(child_win),
            _embedded_view: None,
            is_child_window: true,
            parent_ns_window,
            editor_width: width,
            editor_height: height,
        }, width, height));
    }

    #[cfg(not(target_os = "macos"))]
    bail!("VST3 child window editor not yet supported on this platform")
}

/// Open the VST3 editor in a borderless child window (standalone mode — no engine context).
///
/// Same as `open_vst3_editor_child_window` but loads a fresh plugin instance.
pub fn open_vst3_editor_child_window_standalone(
    bundle_path: &Path,
    uid: &[u8; 16],
    plugin_name: &str,
    sample_rate: f64,
    parent_ns_window: *mut std::ffi::c_void,
    x: f64,
    y: f64,
) -> Result<(Vst3EditorHandle, f64, f64)> {
    let plugin = Vst3Plugin::load(bundle_path, uid, sample_rate, 2, 512, &[])?;

    unsafe {
        use vst3::Steinberg::Vst::IConnectionPoint;
        use vst3::Steinberg::Vst::IConnectionPointTrait;
        if let Some(comp_cp) = plugin.component().cast::<IConnectionPoint>() {
            if let Some(ctrl_cp) = plugin.controller().cast::<IConnectionPoint>() {
                let _ = comp_cp.connect(ctrl_cp.as_ptr());
                let _ = ctrl_cp.connect(comp_cp.as_ptr());
            }
        }
    }

    let controller = plugin.controller_clone();

    let view_ptr = unsafe { controller.createView(vst3::Steinberg::Vst::ViewType::kEditor) };
    if view_ptr.is_null() {
        bail!("plugin '{}' returned null IPlugView (no GUI)", plugin_name);
    }

    #[cfg(target_os = "macos")]
    {
        use vst3::Steinberg::{kPlatformTypeNSView, kResultOk, ViewRect};

        let library = plugin.library_arc();

        let component_handler = {
            let wrapper = ComponentHandler::new(crate::param_channel::vst3_param_channel()).into_com_ptr();
            unsafe {
                use vst3::Steinberg::Vst::IComponentHandler;
                if let Some(com_ref) = wrapper.as_com_ref::<IComponentHandler>() {
                    let _ = controller.setComponentHandler(com_ref.as_ptr());
                }
            }
            Some(wrapper)
        };

        let view: ComPtr<vst3::Steinberg::IPlugView> =
            unsafe { ComPtr::from_raw_unchecked(view_ptr) };

        let res = unsafe { view.isPlatformTypeSupported(kPlatformTypeNSView) };
        if res != kResultOk {
            bail!("plugin '{}' does not support NSView GUI (result={})", plugin_name, res);
        }

        let mut rect = ViewRect { left: 0, top: 0, right: 800, bottom: 600 };
        unsafe { view.getSize(&mut rect) };
        let width = (rect.right - rect.left).max(200) as f64;
        let height = (rect.bottom - rect.top).max(100) as f64;

        let child_win = macos::create_borderless_child_window(parent_ns_window, width, height)?;

        let (sx, sy) = macos::window_to_screen_coords(parent_ns_window, x, y, height);
        child_win.set_frame_origin(sx, sy);

        let content_view = child_win.content_view();
        let res = unsafe { view.attached(content_view, kPlatformTypeNSView) };
        if res != kResultOk {
            bail!("IPlugView::attached failed (result={})", res);
        }

        child_win.set_visible(true);

        return Ok((Vst3EditorHandle {
            view,
            _library: library,
            _controller: controller,
            _component_handler: component_handler,
            _standalone_plugin: Some(Box::new(plugin)),
            _ns_window: Some(child_win),
            _embedded_view: None,
            is_child_window: true,
            parent_ns_window,
            editor_width: width,
            editor_height: height,
        }, width, height));
    }

    #[cfg(not(target_os = "macos"))]
    bail!("VST3 child window editor not yet supported on this platform")
}

// ---------------------------------------------------------------------------
// macOS implementation
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod macos {
    use anyhow::Result;
    use std::ffi::{c_void, CString};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGSize {
        width: f64,
        height: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSRect {
        origin: CGPoint,
        size: CGSize,
    }

    impl NSRect {
        fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
            NSRect {
                origin: CGPoint { x, y },
                size: CGSize { width: w, height: h },
            }
        }
    }

    // NSWindowStyleMask
    const NS_TITLED: u64 = 1 << 0;
    const NS_CLOSABLE: u64 = 1 << 1;
    const NS_MINIATURIZABLE: u64 = 1 << 2;
    const NS_BACKING_BUFFERED: u64 = 2;

    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(name: *const i8) -> *mut c_void;
        fn sel_registerName(str_: *const i8) -> *mut c_void;
        fn objc_msgSend();
    }

    #[link(name = "AppKit", kind = "framework")]
    extern "C" {}

    // Helper: get a selector by name.
    pub(crate) fn sel(name: &str) -> *mut c_void {
        let s = CString::new(name).unwrap();
        unsafe { sel_registerName(s.as_ptr()) }
    }

    // Helper: get a class by name.
    pub(crate) fn cls(name: &str) -> *mut c_void {
        let s = CString::new(name).unwrap();
        unsafe { objc_getClass(s.as_ptr()) }
    }

    // Typed casts of objc_msgSend for different call shapes.
    type MsgSend0 = unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void;
    type MsgSend1V = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> *mut c_void;
    type MsgSend1R =
        unsafe extern "C" fn(*mut c_void, *mut c_void, NSRect, u64, u64, bool) -> *mut c_void;
    type MsgSend1I64 = unsafe extern "C" fn(*mut c_void, *mut c_void, i64) -> *mut c_void;
    type MsgSendBool = unsafe extern "C" fn(*mut c_void, *mut c_void, bool) -> *mut c_void;

    macro_rules! msg0 {
        ($obj:expr, $sel:expr) => {{
            let f: MsgSend0 = std::mem::transmute(objc_msgSend as *const ());
            f($obj, $sel)
        }};
    }

    macro_rules! msg1v {
        ($obj:expr, $sel:expr, $arg:expr) => {{
            let f: MsgSend1V = std::mem::transmute(objc_msgSend as *const ());
            f($obj, $sel, $arg)
        }};
    }

    macro_rules! msg1i {
        ($obj:expr, $sel:expr, $arg:expr) => {{
            let f: MsgSend1I64 = std::mem::transmute(objc_msgSend as *const ());
            f($obj, $sel, $arg)
        }};
    }

    macro_rules! msg_bool {
        ($obj:expr, $sel:expr, $arg:expr) => {{
            let f: MsgSendBool = std::mem::transmute(objc_msgSend as *const ());
            f($obj, $sel, $arg)
        }};
    }

    macro_rules! msg_init_window {
        ($obj:expr, $rect:expr, $style:expr) => {{
            let f: MsgSend1R = std::mem::transmute(objc_msgSend as *const ());
            f($obj, sel("initWithContentRect:styleMask:backing:defer:"), $rect, $style, NS_BACKING_BUFFERED, false)
        }};
    }

    /// An NSView that is removed from its superview and released in Drop.
    pub struct OwnedNsView(*mut c_void);

    unsafe impl Send for OwnedNsView {}

    impl OwnedNsView {
        pub fn ptr(&self) -> *mut c_void {
            self.0
        }

        /// Show or hide this view.
        pub fn set_hidden(&self, hidden: bool) {
            unsafe {
                type MsgSendBool = unsafe extern "C" fn(*mut c_void, *mut c_void, bool) -> *mut c_void;
                let f: MsgSendBool = std::mem::transmute(objc_msgSend as *const ());
                f(self.0, sel("setHidden:"), hidden);
            }
        }

        /// Reposition this view within its parent.
        ///
        /// `x`, `y` use top-left origin (Slint convention). Automatically
        /// converts to bottom-left if the parent NSView is not flipped.
        pub fn reposition(&self, x: f64, y: f64) {
            unsafe {
                // Get superview
                let superview = msg0!(self.0, sel("superview"));
                if superview.is_null() {
                    return;
                }

                // Get current frame to preserve size
                type MsgSendRect = unsafe extern "C" fn(*mut c_void, *mut c_void) -> NSRect;
                let f: MsgSendRect = std::mem::transmute(objc_msgSend as *const ());
                let current_frame = f(self.0, sel("frame"));
                let width = current_frame.size.width;
                let height = current_frame.size.height;

                // Check if parent is flipped
                type MsgSendBoolRet = unsafe extern "C" fn(*mut c_void, *mut c_void) -> bool;
                let is_flipped_fn: MsgSendBoolRet = std::mem::transmute(objc_msgSend as *const ());
                let is_flipped = is_flipped_fn(superview, sel("isFlipped"));

                let actual_y = if is_flipped {
                    y
                } else {
                    let parent_bounds = f(superview, sel("bounds"));
                    parent_bounds.size.height - y - height
                };

                let new_frame = NSRect::new(x, actual_y, width, height);
                type MsgSendSetFrame = unsafe extern "C" fn(*mut c_void, *mut c_void, NSRect);
                let set_f: MsgSendSetFrame = std::mem::transmute(objc_msgSend as *const ());
                set_f(self.0, sel("setFrame:"), new_frame);
            }
        }
    }

    impl Drop for OwnedNsView {
        fn drop(&mut self) {
            unsafe {
                // Remove from superview
                msg0!(self.0, sel("removeFromSuperview"));
                // Release
                let f: unsafe extern "C" fn(*mut c_void, *mut c_void) =
                    std::mem::transmute(objc_msgSend as *const ());
                f(self.0, sel("release"));
            }
        }
    }

    /// Create a child NSView inside a parent NSView at a given position.
    ///
    /// `x`, `y` are in the parent's coordinate system. On macOS the Slint
    /// content view is typically **flipped** (y=0 at top), so pass Slint
    /// logical coordinates directly.
    pub fn create_child_nsview(parent: *mut c_void, x: f64, y: f64, width: f64, height: f64) -> Result<OwnedNsView> {
        unsafe {
            // Check if the parent view is flipped. If not, convert y.
            type MsgSendBoolRet = unsafe extern "C" fn(*mut c_void, *mut c_void) -> bool;
            let is_flipped_fn: MsgSendBoolRet = std::mem::transmute(objc_msgSend as *const ());
            let is_flipped = is_flipped_fn(parent, sel("isFlipped"));

            let actual_y = if is_flipped {
                y
            } else {
                // Need parent's height to flip: ns_y = parent_height - y - height
                type MsgSendRect = unsafe extern "C" fn(*mut c_void, *mut c_void) -> NSRect;
                let f: MsgSendRect = std::mem::transmute(objc_msgSend as *const ());
                let parent_bounds = f(parent, sel("bounds"));
                parent_bounds.size.height - y - height
            };

            let ns_view_cls = cls("NSView");
            let alloc = msg0!(ns_view_cls, sel("alloc"));
            if alloc.is_null() {
                anyhow::bail!("NSView alloc returned nil");
            }

            let frame = NSRect::new(x, actual_y, width, height);
            let init_sel = sel("initWithFrame:");
            type MsgSendFrame = unsafe extern "C" fn(*mut c_void, *mut c_void, NSRect) -> *mut c_void;
            let f: MsgSendFrame = std::mem::transmute(objc_msgSend as *const ());
            let child = f(alloc, init_sel, frame);
            if child.is_null() {
                anyhow::bail!("NSView initWithFrame: returned nil");
            }

            // Add as subview of parent
            msg1v!(parent, sel("addSubview:"), child);

            Ok(OwnedNsView(child))
        }
    }

    /// An NSWindow that is released in Drop.
    pub struct OwnedNsWindow(*mut c_void);

    unsafe impl Send for OwnedNsWindow {}

    impl OwnedNsWindow {
        pub fn content_view(&self) -> *mut c_void {
            unsafe { msg0!(self.0, sel("contentView")) }
        }

        /// Register this window as a child of `parent` so it always floats
        /// above the parent and moves with it.
        pub fn add_as_child_of(&self, parent: *mut c_void) {
            unsafe {
                // NSWindowOrderingMode.above = 1
                type MsgSendChild = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, i64) -> *mut c_void;
                let f: MsgSendChild = std::mem::transmute(objc_msgSend as *const ());
                f(parent, sel("addChildWindow:ordered:"), self.0, 1);
            }
        }

        /// Position this window at screen coordinates `(x, y)` (bottom-left origin).
        pub fn set_frame_origin(&self, x: f64, y: f64) {
            unsafe {
                #[repr(C)]
                struct NSPoint { x: f64, y: f64 }
                type MsgSendPoint = unsafe extern "C" fn(*mut c_void, *mut c_void, NSPoint);
                let f: MsgSendPoint = std::mem::transmute(objc_msgSend as *const ());
                f(self.0, sel("setFrameOrigin:"), NSPoint { x, y });
            }
        }

        /// Show or hide this window.
        pub fn set_visible(&self, visible: bool) {
            unsafe {
                if visible {
                    msg1v!(self.0, sel("orderFront:"), std::ptr::null_mut());
                } else {
                    msg1v!(self.0, sel("orderOut:"), std::ptr::null_mut());
                }
            }
        }

        pub fn show(&self, title: &str) {
            unsafe {
                // Set title
                let ns_str_cls = cls("NSString");
                let title_cstr = CString::new(title).unwrap();
                let ns_title =
                    msg1v!(ns_str_cls, sel("stringWithUTF8String:"), title_cstr.as_ptr() as *mut c_void);
                msg1v!(self.0, sel("setTitle:"), ns_title);

                // Make key and order front
                msg1v!(self.0, sel("makeKeyAndOrderFront:"), std::ptr::null_mut());
                // Raise to front
                msg0!(self.0, sel("orderFront:"));
            }
        }
    }

    impl Drop for OwnedNsWindow {
        fn drop(&mut self) {
            unsafe {
                // Close and release
                msg0!(self.0, sel("close"));
                // NSWindow is normally released by the system when closed,
                // but we set releasedWhenClosed:NO so we need to release manually.
                // Just call orderOut: to hide it safely.
                // The actual memory is managed by ObjC ARC / retain-release.
                // Setting releasedWhenClosed:NO means the window object lives
                // until we stop referencing it. We release our reference here
                // by calling `release`.
                let release_sel = sel("release");
                let f: unsafe extern "C" fn(*mut c_void, *mut c_void) =
                    std::mem::transmute(objc_msgSend as *const ());
                f(self.0, release_sel);
            }
        }
    }

    /// Convert Slint-style window-relative coordinates (top-left origin) to
    /// macOS screen coordinates (bottom-left origin) for positioning a child
    /// window.
    ///
    /// `parent_ns_window` is the parent NSWindow pointer.
    /// `x`, `y` are logical pixels from the top-left of the parent window.
    /// `child_height` is the height of the child window being positioned.
    pub fn window_to_screen_coords(
        parent_ns_window: *mut c_void,
        x: f64,
        y: f64,
        child_height: f64,
    ) -> (f64, f64) {
        unsafe {
            // Get parent window's frame (screen coords, bottom-left origin).
            type MsgSendRect = unsafe extern "C" fn(*mut c_void, *mut c_void) -> NSRect;
            let f: MsgSendRect = std::mem::transmute(objc_msgSend as *const ());
            let parent_frame = f(parent_ns_window, sel("frame"));

            // Get the parent's content view frame to know the content area offset.
            let content_view = msg0!(parent_ns_window, sel("contentView"));
            let content_frame = f(content_view, sel("frame"));

            // In screen coords: parent's content origin is at
            // (parent_frame.origin.x, parent_frame.origin.y + title_bar_offset)
            // where title_bar_offset = parent_frame.height - content_frame.height
            let title_bar_height = parent_frame.size.height - content_frame.size.height;

            // Convert top-left y to bottom-left:
            // screen_y = parent_top - y - child_height
            // parent_top = parent_frame.origin.y + parent_frame.size.height - title_bar_height
            let parent_content_top = parent_frame.origin.y + parent_frame.size.height - title_bar_height;
            let screen_x = parent_frame.origin.x + x;
            let screen_y = parent_content_top - y - child_height;

            (screen_x, screen_y)
        }
    }

    #[link(name = "objc")]
    extern "C" {
        fn objc_allocateClassPair(superclass: *mut c_void, name: *const i8, extra_bytes: usize) -> *mut c_void;
        fn objc_registerClassPair(cls: *mut c_void);
        fn class_addMethod(cls: *mut c_void, sel: *mut c_void, imp: *const c_void, types: *const i8) -> bool;
    }

    /// Return YES from canBecomeKeyWindow so the borderless window accepts
    /// key focus and mouse events stay local (not forwarded to the parent winit window).
    extern "C" fn can_become_key_window(_this: *mut c_void, _sel: *mut c_void) -> bool {
        true
    }

    /// Get or create the `OpenRigBorderlessWindow` runtime ObjC class.
    ///
    /// This is an NSWindow subclass that overrides `canBecomeKeyWindow` → YES.
    /// Borderless windows (style mask 0) return NO by default, which causes
    /// mouse events to be forwarded to the key window (winit's window), triggering panics.
    fn borderless_window_class() -> *mut c_void {
        use std::sync::Once;
        static REGISTER: Once = Once::new();
        static mut CLASS: *mut c_void = std::ptr::null_mut();

        REGISTER.call_once(|| {
            unsafe {
                let superclass = cls("NSWindow");
                let name = CString::new("OpenRigBorderlessWindow").unwrap();
                let new_cls = objc_allocateClassPair(superclass, name.as_ptr(), 0);
                if !new_cls.is_null() {
                    let types = CString::new("B@:").unwrap(); // BOOL, self, _cmd
                    class_addMethod(
                        new_cls,
                        sel("canBecomeKeyWindow"),
                        can_become_key_window as *const c_void,
                        types.as_ptr(),
                    );
                    objc_registerClassPair(new_cls);
                    CLASS = new_cls;
                } else {
                    // Already registered from a previous load — look it up.
                    CLASS = cls("OpenRigBorderlessWindow");
                }
            }
        });
        unsafe { CLASS }
    }

    /// Create a borderless NSWindow and add it as a child of the parent NSWindow.
    ///
    /// Uses a custom NSWindow subclass (`OpenRigBorderlessWindow`) that overrides
    /// `canBecomeKeyWindow` → YES so mouse events stay local and don't get
    /// forwarded to the parent winit window.
    pub fn create_borderless_child_window(
        parent_ns_window: *mut c_void,
        width: f64,
        height: f64,
    ) -> Result<OwnedNsWindow> {
        unsafe {
            let ns_app_cls = cls("NSApplication");
            msg0!(ns_app_cls, sel("sharedApplication"));

            let window_cls = borderless_window_class();
            let window_alloc = msg0!(window_cls, sel("alloc"));
            if window_alloc.is_null() {
                anyhow::bail!("OpenRigBorderlessWindow alloc returned nil");
            }

            // Borderless window (style mask = 0)
            let frame = NSRect::new(0.0, 0.0, width, height);
            let window = msg_init_window!(window_alloc, frame, 0u64);
            if window.is_null() {
                anyhow::bail!("OpenRigBorderlessWindow init returned nil");
            }

            msg_bool!(window, sel("setReleasedWhenClosed:"), false);
            // Non-opaque with clear background so the plugin draws its own content.
            msg_bool!(window, sel("setOpaque:"), false);

            // Add as child of parent so it floats above and moves with it.
            // NSWindowOrderingMode.above = 1
            type MsgSendChild = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, i64) -> *mut c_void;
            let f: MsgSendChild = std::mem::transmute(objc_msgSend as *const ());
            f(parent_ns_window, sel("addChildWindow:ordered:"), window, 1);

            Ok(OwnedNsWindow(window))
        }
    }

    /// Create a native NSWindow for the plugin GUI.
    pub fn create_editor_window(plugin_name: &str, width: f64, height: f64) -> Result<OwnedNsWindow> {
        unsafe {
            // Ensure NSApp is initialized (it should be since Slint is running).
            let ns_app_cls = cls("NSApplication");
            msg0!(ns_app_cls, sel("sharedApplication"));

            let ns_window_cls = cls("NSWindow");
            let alloc_sel = sel("alloc");
            let window_alloc = msg0!(ns_window_cls, alloc_sel);
            if window_alloc.is_null() {
                anyhow::bail!("NSWindow alloc returned nil");
            }

            let frame = NSRect::new(200.0, 200.0, width, height);
            let style = NS_TITLED | NS_CLOSABLE | NS_MINIATURIZABLE;
            let window = msg_init_window!(window_alloc, frame, style);
            if window.is_null() {
                anyhow::bail!("NSWindow init returned nil");
            }

            // Do not release when closed — we manage lifetime via Drop.
            msg_bool!(window, sel("setReleasedWhenClosed:"), false);

            // Set window level: floating (so it stays above the main window).
            // NSFloatingWindowLevel = 3
            msg1i!(window, sel("setLevel:"), 3);

            // Center on screen.
            msg0!(window, sel("center"));

            // Title (set again during show(), but set here too).
            let title_cstr = CString::new(plugin_name).unwrap_or_default();
            let ns_str_cls = cls("NSString");
            let ns_title = msg1v!(
                ns_str_cls,
                sel("stringWithUTF8String:"),
                title_cstr.as_ptr() as *mut c_void
            );
            msg1v!(window, sel("setTitle:"), ns_title);

            Ok(OwnedNsWindow(window))
        }
    }
}
