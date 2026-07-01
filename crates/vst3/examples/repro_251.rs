//! #251 reproduction/verification harness. This class of bug can't be a normal
//! `#[test]`: the failure needs the process **main thread** to run an
//! NSApplication + CFRunLoop (like the app's winit/Slint loop) while the plugin
//! is instantiated on a background stream thread — a cargo test runs on a worker
//! thread and can't own the main run loop.
//!
//! Before the `bundleEntry` fix, `bg-nsapp-loop` reproduced the app's
//! `createInstance result=-1`; with it, every mode loads OK.
//!
//! Usage: cargo run -p vst3-host --example repro_251 -- <mode>
//!   baseline        main thread, no NSApp, no run loop
//!   bg              background thread, no NSApp
//!   bg-nsapp        background thread, NSApp initialised, NO run loop
//!   bg-nsapp-loop   background thread, NSApp + main run loop running (app-like)

use std::ffi::c_void;
use std::sync::mpsc;

const MODEL_ID: &str = "vst3:ValhallaSupermassive:ValhallaSupermassive";

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRunLoopGetMain() -> *mut c_void;
    fn CFRunLoopRun();
    fn CFRunLoopStop(rl: *mut c_void);
}
#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const i8) -> *mut c_void;
    fn sel_registerName(s: *const i8) -> *mut c_void;
    fn objc_msgSend();
}

fn load_once(tag: &str) {
    vst3_host::init_vst3_catalog(48_000.0);
    let Some(entry) = vst3_host::find_vst3_plugin(MODEL_ID) else {
        println!("[{tag}] plugin not installed");
        return;
    };
    let uid = vst3_host::resolve_uid_for_model(MODEL_ID).unwrap();
    match vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, 48_000.0, 2, 512, &[]) {
        Ok(_) => println!("[{tag}] load OK"),
        Err(e) => println!("[{tag}] load ERR: {e}"),
    }
}

fn nsapp_init() {
    unsafe {
        let cls = objc_getClass(b"NSApplication\0".as_ptr() as *const i8);
        let sel = sel_registerName(b"sharedApplication\0".as_ptr() as *const i8);
        let f: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        f(cls, sel);
    }
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "baseline".into());
    match mode.as_str() {
        "baseline" => load_once("baseline main"),
        "bg" => {
            std::thread::spawn(|| load_once("bg")).join().unwrap();
        }
        "bg-nsapp" => {
            nsapp_init();
            std::thread::spawn(|| load_once("bg+nsapp")).join().unwrap();
        }
        "bg-nsapp-loop" => {
            nsapp_init();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                load_once("bg+nsapp+loop");
                tx.send(()).unwrap();
                unsafe { CFRunLoopStop(CFRunLoopGetMain()) };
            });
            unsafe { CFRunLoopRun() };
            let _ = rx.recv();
        }
        other => println!("unknown mode: {other}"),
    }
}
