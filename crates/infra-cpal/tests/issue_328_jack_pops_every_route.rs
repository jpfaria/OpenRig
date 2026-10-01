//! #328 — the Linux/JACK-direct callback (`jack_handlers.rs`) and client
//! builder (`jack_direct.rs`), compiled only with
//! `cfg(all(target_os = "linux", feature = "jack"))`, must serve every output
//! route of the chain's runtime on that route's own ports. Popping route 0
//! alone left a Y → A/B chain's path-B output silent on the Orange Pi. Pinned
//! by source so every platform's test run guards the JACK-only files.

use std::path::Path;

fn source(file: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(file))
        .unwrap_or_else(|e| panic!("read {file}: {e}"))
}

#[test]
fn the_jack_callback_plays_every_route() {
    let handlers = source("jack_handlers.rs");
    assert!(
        !handlers.contains("process_output_f32(&self.runtime, 0,"),
        "#328: the JACK callback still pops route 0 only"
    );
    assert!(
        handlers.contains("process_output_f32(runtime, output.route,"),
        "#328: the JACK callback must pop each route into that route's own ports"
    );
}

#[test]
fn the_jack_client_registers_ports_per_route() {
    let direct = source("jack_direct.rs");
    assert!(
        direct.contains("route_ports(runtime.output_route_count(), max_out_ch)"),
        "#328: the JACK client must register one port set per output route"
    );
}
