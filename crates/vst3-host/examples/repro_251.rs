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
    fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, return_after: u8) -> i32;
    static kCFRunLoopDefaultMode: *const c_void;
}

/// Pump the current thread's run loop for `secs` (service JUCE message thread).
fn pump(secs: f64) {
    unsafe {
        CFRunLoopRunInMode(kCFRunLoopDefaultMode, secs, 0);
    }
}
#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const i8) -> *mut c_void;
    fn sel_registerName(s: *const i8) -> *mut c_void;
    fn objc_msgSend();
}

fn load_keep(tag: &str) -> Option<vst3_host::Vst3Plugin> {
    vst3_host::init_vst3_catalog(48_000.0);
    let Some(entry) = vst3_host::find_vst3_plugin(MODEL_ID) else {
        println!("[{tag}] plugin not installed");
        return None;
    };
    let uid = vst3_host::resolve_uid_for_model(MODEL_ID).unwrap();
    match vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, 48_000.0, 2, 512, &[]) {
        Ok(p) => {
            println!("[{tag}] load OK");
            Some(p)
        }
        Err(e) => {
            println!("[{tag}] load ERR: {e}");
            None
        }
    }
}

fn load_once(tag: &str) {
    // dropped immediately at end of scope
    let _ = load_keep(tag);
}

unsafe fn msg0(obj: *mut c_void, sel: &[u8]) -> *mut c_void {
    let s = sel_registerName(sel.as_ptr() as *const i8);
    let f: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
        std::mem::transmute(objc_msgSend as *const ());
    f(obj, s)
}
unsafe fn msg1i(obj: *mut c_void, sel: &[u8], arg: i64) -> *mut c_void {
    let s = sel_registerName(sel.as_ptr() as *const i8);
    let f: unsafe extern "C" fn(*mut c_void, *mut c_void, i64) -> *mut c_void =
        std::mem::transmute(objc_msgSend as *const ());
    f(obj, s, arg)
}

fn nsapp() -> *mut c_void {
    unsafe {
        let cls = objc_getClass(b"NSApplication\0".as_ptr() as *const i8);
        msg0(cls, b"sharedApplication\0")
    }
}

/// Bring NSApp fully up like a real GUI app (what winit/Slint does).
fn nsapp_full() -> *mut c_void {
    unsafe {
        let app = nsapp();
        // NSApplicationActivationPolicyRegular = 0
        msg1i(app, b"setActivationPolicy:\0", 0);
        msg0(app, b"finishLaunching\0");
        app
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
            nsapp_full();
            std::thread::spawn(|| load_once("bg+nsapp")).join().unwrap();
        }
        // Two sequential loads ON THE MAIN THREAD under NSApp.
        "main-nsapp-2" => {
            nsapp_full();
            let _k1 = load_keep("main #1");
            load_once("main #2 (while #1 alive)");
        }
        // Candidate fix: warm up JUCE with one instance on the MAIN thread first
        // (kept alive), then load more on background threads.
        "warm-main" => {
            let _warm = load_keep("warm (main thread)");
            nsapp_full();
            std::thread::spawn(|| {
                load_once("bg #1 after warm");
                load_once("bg #2 after warm");
            })
            .join()
            .unwrap();
        }
        // NSApp initialised but its run loop is NOT running; two loads.
        "nsapp-noloop-2" => {
            nsapp_full();
            std::thread::spawn(|| {
                let _k1 = load_keep("noloop #1");
                load_once("noloop #2 (while #1 alive)");
            })
            .join()
            .unwrap();
        }
        "bg-nsapp-loop" => {
            nsapp_full();
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
        // Candidate fix: pump the loader thread's run loop between instances.
        "bg-pump" => {
            let app = nsapp_full();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                std::thread::spawn(|| {
                    let _k1 = load_keep("pump #1");
                    pump(0.3);
                    let _k2 = load_keep("pump #2");
                    pump(0.3);
                    let _k3 = load_keep("pump #3");
                })
                .join()
                .unwrap();
                std::process::exit(0);
            });
            unsafe { msg0(app, b"run\0") };
        }
        // Isolate the drop: keep #1 ALIVE, then load #2 on the same thread.
        "bg-keep" => {
            let app = nsapp_full();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                std::thread::spawn(|| {
                    let _keep = load_keep("keep #1 (alive)");
                    load_once("load #2 (while #1 alive)");
                    drop(_keep);
                })
                .join()
                .unwrap();
                std::process::exit(0);
            });
            unsafe { msg0(app, b"run\0") };
        }
        // Candidate fix: one dedicated thread loads twice — must both be OK.
        "bg-same-thread-twice" => {
            let app = nsapp_full();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                std::thread::spawn(|| {
                    load_once("same-thread #1");
                    load_once("same-thread #2");
                })
                .join()
                .unwrap();
                std::process::exit(0);
            });
            unsafe { msg0(app, b"run\0") };
        }
        // App-like: NSApp running, FOUR streams load concurrently (barrier so
        // their createInstance truly overlap — the owner's 4-mono-input rig).
        "bg-concurrent-4" => {
            use std::sync::{Arc, Barrier};
            let app = nsapp_full();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                let barrier = Arc::new(Barrier::new(4));
                let handles: Vec<_> = (0..4)
                    .map(|i| {
                        let b = barrier.clone();
                        std::thread::spawn(move || {
                            b.wait();
                            load_once(&format!("stream-{i}"));
                        })
                    })
                    .collect();
                for h in handles {
                    h.join().unwrap();
                }
                std::process::exit(0);
            });
            unsafe { msg0(app, b"run\0") };
        }
        // Staggered overlap: start 4 loads 60ms apart so they overlap in flight
        // but only the first initialises JUCE (others just bump the refcount).
        "bg-stagger-4" => {
            let app = nsapp_full();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                let handles: Vec<_> = (0..4)
                    .map(|i| {
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(60 * i));
                            load_once(&format!("stream-{i}"));
                        })
                    })
                    .collect();
                for h in handles {
                    h.join().unwrap();
                }
                std::process::exit(0);
            });
            unsafe { msg0(app, b"run\0") };
        }
        // App-like: NSApp running, TWO stream threads load concurrently.
        "bg-concurrent" => {
            let app = nsapp_full();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                let a = std::thread::spawn(|| load_once("stream-0"));
                let b = std::thread::spawn(|| load_once("stream-1"));
                a.join().unwrap();
                b.join().unwrap();
                std::process::exit(0);
            });
            unsafe { msg0(app, b"run\0") };
        }
        // App-like: NSApp running, load twice on two SEPARATE threads in turn.
        "bg-two-threads" => {
            let app = nsapp_full();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                std::thread::spawn(|| load_once("thread-A")).join().unwrap();
                std::thread::spawn(|| load_once("thread-B")).join().unwrap();
                std::process::exit(0);
            });
            unsafe { msg0(app, b"run\0") };
        }
        // Closest to the app: a real running NSApp ([NSApp run]) while the
        // plugin loads on a background thread. Exit the process from the bg
        // thread once done (stopping [NSApp run] cleanly needs an event).
        "bg-nsapp-run" => {
            let app = nsapp_full();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                load_once("bg+nsapp+run");
                std::process::exit(0);
            });
            unsafe { msg0(app, b"run\0") };
        }
        other => println!("unknown mode: {other}"),
    }
}
