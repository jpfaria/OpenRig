// Headless Slint renderer: compile a .slint at runtime (interpreter) and
// render a component to a PNG via the software renderer — no display server,
// no app code. Lets the agent self-verify UI layout.
//
// usage: slint-render <file.slint> <Component> <out.png> [width] [height]

use std::rc::Rc;

use image::{Rgba, RgbaImage};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel};
use slint::platform::{Platform, PlatformError, WindowAdapter};
use slint::PhysicalSize;
use slint_interpreter::{Compiler, ComponentHandle};

struct SwPlatform {
    window: Rc<MinimalSoftwareWindow>,
}

impl Platform for SwPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: slint-render <file.slint> <Component> <out.png> [width] [height]");
        std::process::exit(2);
    }
    let path = args[1].clone();
    let component = args[2].clone();
    let out = args[3].clone();
    let width: u32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(900);
    let height: u32 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(900);

    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(SwPlatform { window: window.clone() }))
        .expect("set_platform");

    // Fonts: a .slint can `import "path/to/font.ttf";` and the interpreter
    // registers it at runtime (needs slint's software-renderer-systemfonts
    // feature, enabled in Cargo.toml). No OS font install, no env hacks.

    let compiler = Compiler::default();
    let result = spin_on::spin_on(compiler.build_from_path(&path));
    let mut had_error = false;
    for d in result.diagnostics() {
        eprintln!("{d}");
        if d.level() == slint_interpreter::DiagnosticLevel::Error {
            had_error = true;
        }
    }
    if had_error {
        std::process::exit(1);
    }
    let def = match result.component(&component) {
        Some(d) => d,
        None => {
            eprintln!("component '{component}' not found in {path}");
            std::process::exit(1);
        }
    };
    let instance = def.create().expect("create instance");
    instance.show().expect("show");

    window.set_size(PhysicalSize::new(width, height));

    let mut buffer = vec![Rgb565Pixel(0); (width * height) as usize];
    for _ in 0..3 {
        slint::platform::update_timers_and_animations();
        instance.window().request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.render(&mut buffer, width as usize);
        });
    }

    let mut img = RgbaImage::new(width, height);
    for (i, px) in buffer.iter().enumerate() {
        let v = px.0;
        let r5 = ((v >> 11) & 0x1f) as u32;
        let g6 = ((v >> 5) & 0x3f) as u32;
        let b5 = (v & 0x1f) as u32;
        let r = ((r5 * 255 + 15) / 31) as u8;
        let g = ((g6 * 255 + 31) / 63) as u8;
        let b = ((b5 * 255 + 15) / 31) as u8;
        img.put_pixel((i as u32) % width, (i as u32) / width, Rgba([r, g, b, 255]));
    }
    img.save(&out).expect("save png");
    eprintln!("wrote {out} ({width}x{height})");
}
