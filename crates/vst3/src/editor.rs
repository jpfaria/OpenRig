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

impl block_core::PluginEditorHandle for Vst3EditorHandle {}

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
        });
    }

    #[cfg(not(target_os = "macos"))]
    bail!("VST3 editor window not yet supported on this platform")
}

/// Open the VST3 editor embedded inside a parent window (no new OS window).
///
/// `parent_ns_view` must be a valid `NSView*` pointer (e.g. from Slint's raw
/// window handle). The plugin view is attached to a child NSView sized to the
/// plugin's requested dimensions, positioned at (0, 0) within the parent.
///
/// Must be called on the main/UI thread.
pub fn open_vst3_editor_embedded(
    plugin_name: &str,
    gui_context: Vst3GuiContext,
    parent_ns_view: *mut std::ffi::c_void,
) -> Result<Vst3EditorHandle> {
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

        let child_view = macos::create_child_nsview(parent_ns_view, width, height)?;
        let ns_view_ptr = child_view.ptr();

        let res = unsafe { view.attached(ns_view_ptr, kPlatformTypeNSView) };
        if res != kResultOk {
            bail!("IPlugView::attached failed (result={})", res);
        }

        return Ok(Vst3EditorHandle {
            view,
            _library: library,
            _controller: controller,
            _component_handler: component_handler,
            _standalone_plugin: None,
            _ns_window: None,
            _embedded_view: Some(child_view),
        });
    }

    #[cfg(not(target_os = "macos"))]
    bail!("VST3 embedded editor not yet supported on this platform")
}

/// Open the VST3 editor embedded (standalone mode — no engine context).
pub fn open_vst3_editor_embedded_standalone(
    bundle_path: &Path,
    uid: &[u8; 16],
    plugin_name: &str,
    sample_rate: f64,
    parent_ns_view: *mut std::ffi::c_void,
) -> Result<Vst3EditorHandle> {
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

        let child_view = macos::create_child_nsview(parent_ns_view, width, height)?;
        let ns_view_ptr = child_view.ptr();

        let res = unsafe { view.attached(ns_view_ptr, kPlatformTypeNSView) };
        if res != kResultOk {
            bail!("IPlugView::attached failed (result={})", res);
        }

        return Ok(Vst3EditorHandle {
            view,
            _library: library,
            _controller: controller,
            _component_handler: component_handler,
            _standalone_plugin: Some(Box::new(plugin)),
            _ns_window: None,
            _embedded_view: Some(child_view),
        });
    }

    #[cfg(not(target_os = "macos"))]
    bail!("VST3 embedded editor not yet supported on this platform")
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

    /// Create a child NSView inside a parent NSView, sized to fit the plugin.
    pub fn create_child_nsview(parent: *mut c_void, width: f64, height: f64) -> Result<OwnedNsView> {
        unsafe {
            let ns_view_cls = cls("NSView");
            let alloc = msg0!(ns_view_cls, sel("alloc"));
            if alloc.is_null() {
                anyhow::bail!("NSView alloc returned nil");
            }

            let frame = NSRect::new(0.0, 0.0, width, height);
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
