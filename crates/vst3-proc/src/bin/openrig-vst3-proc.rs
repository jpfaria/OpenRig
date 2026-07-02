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

    // Instances are created lazily, on demand: no NSApplication in this process
    // → sequential createInstance is reliable, unlike inside the GUI app (#251).
    let cap = n.min(vst3_proc::MAX_INSTANCES);
    let mut procs: Vec<Option<StereoVst3Processor>> = (0..cap).map(|_| None).collect();
    shared.ready.store(1, Ordering::SeqCst);

    // Track last-serviced request and last-applied param per slot.
    let mut last_done = vec![0u64; cap];
    let mut last_param = vec![0u64; cap];
    let mut scratch: Vec<[f32; 2]> = vec![[0.0; 2]; MAX_FRAMES];

    loop {
        if shared.shutdown.load(Ordering::SeqCst) != 0 {
            return Ok(());
        }
        let mut did_work = false;
        for i in 0..cap {
            let slot = &shared.slots[i];

            // Lazily create this slot's instance when the parent requests it.
            if slot.load_req.load(Ordering::Acquire) == 1 && procs[i].is_none() {
                match Vst3Plugin::load(bundle, &uid, sample_rate, 2, block_size, &[]) {
                    Ok(p) => {
                        procs[i] = Some(StereoVst3Processor::new(p, None));
                        slot.loaded.store(1, Ordering::Release);
                    }
                    Err(e) => {
                        eprintln!("[vst3-proc] load slot {i} failed: {e:#}");
                        shared.load_failed.store(1, Ordering::SeqCst);
                    }
                }
                did_work = true;
            }
            let Some(proc) = procs[i].as_mut() else {
                continue;
            };

            // Drain any pending parameter changes onto the live instance.
            let pwrite = slot.param_write.load(Ordering::Acquire);
            while last_param[i] < pwrite {
                let (id, val) = slot.read_param(last_param[i]);
                let _ = proc.set_param(id, val as f64);
                last_param[i] += 1;
                did_work = true;
            }

            let req = slot.request.load(Ordering::Acquire);
            if req != last_done[i] {
                did_work = true;
                let frames = (slot.n_frames.load(Ordering::SeqCst) as usize).min(MAX_FRAMES);
                for f in 0..frames {
                    scratch[f] = slot.read_input(f);
                }
                use block_core::StereoProcessor;
                proc.process_block(&mut scratch[..frames]);
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
