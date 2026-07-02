//! Out-of-process VST3 host child. Loads N instances of one plugin (this
//! process has NO NSApplication, so JUCE createInstance works unlimited) and
//! services each slot's request/done handshake over the shared mapping.
//!
//! Args: <shm_path> <bundle_path> <uid_hex> <sample_rate> <block_size> <n>

use std::path::Path;
use std::sync::atomic::Ordering;

use anyhow::{Context, Result};
use vst3_host::{StereoVst3Processor, Vst3Plugin};
use vst3_proc::{parse_uid_hex, Shared, MAX_FRAMES};

fn main() {
    if let Err(e) = run() {
        eprintln!("[vst3-proc] fatal: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 7 {
        anyhow::bail!("usage: {} <shm> <bundle> <uid_hex> <sr> <block> <n>", args[0]);
    }
    let shm_path = Path::new(&args[1]);
    let bundle = Path::new(&args[2]);
    let uid = parse_uid_hex(&args[3])?;
    let sample_rate: f64 = args[4].parse().context("sample_rate")?;
    let block_size: usize = args[5].parse().context("block_size")?;
    let n: usize = args[6].parse().context("n")?;

    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(shm_path)
        .with_context(|| format!("open shm {}", shm_path.display()))?;
    // Safety: the parent sized this file to `Shared` before spawning us.
    let map = unsafe { memmap2::MmapMut::map_mut(&file)? };
    let shared: &Shared = unsafe { Shared::from_ptr(map.as_ptr() as *mut u8) };

    // Create N instances. No NSApplication here → sequential createInstance is
    // reliable, unlike inside the GUI app process (#251).
    let mut procs: Vec<StereoVst3Processor> = Vec::with_capacity(n);
    for i in 0..n {
        let plugin = match Vst3Plugin::load(bundle, &uid, sample_rate, 2, block_size, &[]) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("[vst3-proc] load instance {i} failed: {e:#}");
                shared.load_failed.store(1, Ordering::SeqCst);
                return Err(e);
            }
        };
        procs.push(StereoVst3Processor::new(plugin, None));
        shared.slots[i].loaded.store(1, Ordering::SeqCst);
    }
    shared.ready.store(1, Ordering::SeqCst);

    // Track last-serviced request and last-applied param per slot.
    let mut last_done = vec![0u64; n];
    let mut last_param = vec![0u64; n];
    let mut scratch: Vec<[f32; 2]> = vec![[0.0; 2]; MAX_FRAMES];

    loop {
        if shared.shutdown.load(Ordering::SeqCst) != 0 {
            return Ok(());
        }
        let mut did_work = false;
        for i in 0..n {
            let slot = &shared.slots[i];

            // Apply a pending parameter change to the live instance.
            let pseq = slot.param_seq.load(Ordering::Acquire);
            if pseq != last_param[i] {
                last_param[i] = pseq;
                let id = slot.param_id.load(Ordering::SeqCst);
                let val = f32::from_bits(slot.param_bits.load(Ordering::SeqCst));
                let _ = procs[i].set_param(id, val as f64);
            }

            let req = slot.request.load(Ordering::Acquire);
            if req != last_done[i] {
                did_work = true;
                let frames = (slot.n_frames.load(Ordering::SeqCst) as usize).min(MAX_FRAMES);
                for f in 0..frames {
                    scratch[f] = slot.read_input(f);
                }
                use block_core::StereoProcessor;
                procs[i].process_block(&mut scratch[..frames]);
                for f in 0..frames {
                    slot.write_output(f, scratch[f]);
                }
                slot.done.store(req, Ordering::Release);
                last_done[i] = req;
            }
        }
        if !did_work {
            std::hint::spin_loop();
        }
    }
}
