//! Standalone out-of-process VST3 host child. The real app re-executes itself
//! in child mode (see `vst3_proc::maybe_run_child`); this binary exists so tests
//! (and any external caller) can spawn a dedicated host. Both dispatch through
//! the same `CHILD_FLAG` protocol.

fn main() {
    // Runs the host loop and exits if launched with CHILD_FLAG.
    vst3_proc::maybe_run_child();
    eprintln!("openrig-vst3-proc: only valid when spawned as a host child");
    std::process::exit(2);
}
