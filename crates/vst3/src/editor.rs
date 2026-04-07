//! VST3 editor window: opens the plugin's native GUI in a separate OS window.
//!
//! Calls `IEditController::createView("editor")` to get an `IPlugView`, then
//! creates a native window and embeds the view in it.
//!
//! Must be called on the main/UI thread (macOS AppKit requirement).

use anyhow::{bail, Result};
use std::path::Path;
use vst3::Steinberg::Vst::{IConnectionPointTrait, IEditControllerTrait, ViewType};
use vst3::Steinberg::{IPlugViewTrait, ViewRect, kResultOk};
use vst3::ComPtr;

use crate::host::Vst3Plugin;

impl block_core::PluginEditorHandle for Vst3EditorHandle {}

/// Handle to an open VST3 editor window.
///
/// Closing/dropping this handle calls `IPlugView::removed()` and releases
/// all resources. On macOS it also releases the NSWindow.
pub struct Vst3EditorHandle {
    view: ComPtr<vst3::Steinberg::IPlugView>,
    _plugin: Box<Vst3Plugin>,
    #[cfg(target_os = "macos")]
    _ns_window: macos::OwnedNsWindow,
}

impl Drop for Vst3EditorHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = self.view.removed();
        }
    }
}

/// Open the native editor window for the VST3 plugin identified by
/// `bundle_path` + `uid`.
///
/// Loads a separate plugin instance dedicated to the GUI (not the audio
/// processor). The audio processor is unaffected.
///
/// Returns a `Vst3EditorHandle` that keeps the window alive. Drop it to close.
pub fn open_vst3_editor_window(
    bundle_path: &Path,
    uid: &[u8; 16],
    plugin_name: &str,
    sample_rate: f64,
) -> Result<Vst3EditorHandle> {
    // Load a lightweight plugin instance (2ch, small block) just for the GUI.
    let plugin = Vst3Plugin::load(bundle_path, uid, sample_rate, 2, 512, &[])?;

    // Standard VST3 host requirement: connect component ↔ controller via
    // IConnectionPoint so the controller can query component state before
    // creating its view. Many plugins return null from createView otherwise.
    unsafe {
        use vst3::Steinberg::Vst::IConnectionPoint;
        if let Some(comp_cp) = plugin.component().cast::<IConnectionPoint>() {
            if let Some(ctrl_cp) = plugin.controller().cast::<IConnectionPoint>() {
                let _ = comp_cp.connect(ctrl_cp.as_ptr());
                let _ = ctrl_cp.connect(comp_cp.as_ptr());
                log::debug!("VST3 editor: IConnectionPoint connected");
            }
        }
    }

    // Get IPlugView from the controller.
    let view_ptr = unsafe { plugin.controller().createView(ViewType::kEditor) };
    if view_ptr.is_null() {
        bail!("plugin '{}' returned null IPlugView (no GUI)", plugin_name);
    }
    let view: ComPtr<vst3::Steinberg::IPlugView> =
        unsafe { ComPtr::from_raw_unchecked(view_ptr) };

    // Check that NSView platform is supported.
    #[cfg(target_os = "macos")]
    {
        use vst3::Steinberg::kPlatformTypeNSView;
        let res = unsafe { view.isPlatformTypeSupported(kPlatformTypeNSView) };
        if res != kResultOk {
            bail!("plugin '{}' does not support NSView GUI (result={})", plugin_name, res);
        }
    }

    // Get preferred size.
    let mut rect = ViewRect { left: 0, top: 0, right: 800, bottom: 600 };
    unsafe { view.getSize(&mut rect) };
    let width = (rect.right - rect.left).max(200) as f64;
    let height = (rect.bottom - rect.top).max(100) as f64;

    // Create the native window and embed the view.
    #[cfg(target_os = "macos")]
    {
        use vst3::Steinberg::kPlatformTypeNSView;
        let ns_window = macos::create_editor_window(plugin_name, width, height)?;
        let ns_view = ns_window.content_view();

        let res = unsafe { view.attached(ns_view, kPlatformTypeNSView) };
        if res != kResultOk {
            bail!("IPlugView::attached failed (result={})", res);
        }

        ns_window.show(plugin_name);

        return Ok(Vst3EditorHandle {
            view,
            _plugin: Box::new(plugin),
            _ns_window: ns_window,
        });
    }

    #[cfg(not(target_os = "macos"))]
    bail!("VST3 editor window not yet supported on this platform")
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

    /// An NSWindow that is released in Drop.
    pub struct OwnedNsWindow(*mut c_void);

    unsafe impl Send for OwnedNsWindow {}

    impl OwnedNsWindow {
        pub fn content_view(&self) -> *mut c_void {
            unsafe { msg0!(self.0, sel("contentView")) }
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
