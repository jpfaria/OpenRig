//! Out-of-process VST3 host: shared-memory protocol + parent-side client.
//!
//! A child process (`openrig-vst3-proc`) with no NSApplication loads the VST3
//! instances and processes audio; the parent drives it over a memory-mapped
//! region. See the crate note in `Cargo.toml` for why (#251).

use std::cell::UnsafeCell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use anyhow::{bail, Context, Result};

pub const MAX_INSTANCES: usize = 16;
/// Maximum frames per processing block. Real blocks are far smaller (≤1024).
pub const MAX_FRAMES: usize = 4096;
/// Capacity of the per-slot parameter ring (SPSC parent→child).
pub const PARAM_RING: usize = 256;

/// One plugin instance's mailbox: audio in/out + a request/done handshake and a
/// parameter ring. Interleaved stereo `[L, R]` frames.
#[repr(C)]
pub struct Slot {
    /// Parent bumps this to request processing of `n_frames`.
    pub request: AtomicU64,
    /// Child sets this equal to `request` once the block is processed.
    pub done: AtomicU64,
    pub n_frames: AtomicU32,
    /// Parent sets 1 to ask the child to create this slot's instance.
    pub load_req: AtomicU32,
    /// Child sets 1 once this slot's plugin instance is created.
    pub loaded: AtomicU32,
    /// Total params ever written to the ring (parent-incremented). The child
    /// tracks its own read cursor and drains up to this.
    pub param_write: AtomicU64,
    /// SPSC ring of `(id << 32) | f32::to_bits(value)`.
    param_ring: [UnsafeCell<u64>; PARAM_RING],
    /// Audio buffers. Written/read by whichever side owns the block per the
    /// request/done handshake, so `UnsafeCell` interior mutability is sound.
    input: [UnsafeCell<[f32; 2]>; MAX_FRAMES],
    output: [UnsafeCell<[f32; 2]>; MAX_FRAMES],
}

impl Slot {
    /// Parent: push a `(id, normalized)` param onto the ring.
    pub fn push_param(&self, id: u32, normalized: f32) {
        let w = self.param_write.load(Ordering::Relaxed);
        let packed = ((id as u64) << 32) | normalized.to_bits() as u64;
        // Safety: only the parent writes the ring slot at `w % PARAM_RING`;
        // the child reads it only after observing the bumped `param_write`.
        unsafe { *self.param_ring[(w as usize) % PARAM_RING].get() = packed }
        self.param_write.store(w + 1, Ordering::Release);
    }
    /// Child: read the ring entry at absolute index `idx`.
    pub fn read_param(&self, idx: u64) -> (u32, f32) {
        let packed = unsafe { *self.param_ring[(idx as usize) % PARAM_RING].get() };
        ((packed >> 32) as u32, f32::from_bits(packed as u32))
    }

    #[inline]
    pub fn write_input(&self, i: usize, v: [f32; 2]) {
        // Safety: the parent owns `input` between setting `request` and the
        // child observing it; exclusive by the handshake.
        unsafe { *self.input[i].get() = v }
    }
    #[inline]
    pub fn read_input(&self, i: usize) -> [f32; 2] {
        unsafe { *self.input[i].get() }
    }
    #[inline]
    pub fn write_output(&self, i: usize, v: [f32; 2]) {
        unsafe { *self.output[i].get() = v }
    }
    #[inline]
    pub fn read_output(&self, i: usize) -> [f32; 2] {
        unsafe { *self.output[i].get() }
    }
}

// Safety: all cross-process access is serialised by the atomic request/done
// handshake; only one side touches a given buffer at a time.
unsafe impl Sync for Slot {}
unsafe impl Sync for Shared {}

#[repr(C)]
pub struct Shared {
    /// Child sets 1 once all `n_instances` slots are loaded.
    pub ready: AtomicU32,
    /// Parent sets 1 to ask the child to exit.
    pub shutdown: AtomicU32,
    /// Number of instances the child must create (set by parent before spawn).
    pub n_instances: AtomicU32,
    /// Child sets 1 if it failed to load any instance (parent then bails).
    pub load_failed: AtomicU32,
    pub slots: [Slot; MAX_INSTANCES],
}

impl Shared {
    /// Interpret a mapped region as `&Shared`. The caller guarantees the region
    /// is at least `size_of::<Shared>()` bytes and page-aligned (mmap is).
    ///
    /// # Safety
    /// `ptr` must point to a valid, live mapping of exactly this layout, shared
    /// with the peer process.
    pub unsafe fn from_ptr<'a>(ptr: *mut u8) -> &'a Shared {
        &*(ptr as *const Shared)
    }
}

/// Total bytes the shared mapping needs.
pub fn shared_size() -> usize {
    std::mem::size_of::<Shared>()
}

// ---------------------------------------------------------------------------
// Parent-side client
// ---------------------------------------------------------------------------

/// Drives an out-of-process host of `n` instances of one plugin.
pub struct Vst3ProcClient {
    _file: std::fs::File,
    _map: memmap2::MmapMut,
    /// Base pointer of the mapping (copyable, so methods can read the shared
    /// state without holding a borrow on `self`).
    base: *mut u8,
    child: std::process::Child,
    n: usize,
}

impl Vst3ProcClient {
    #[inline]
    fn shared(&self) -> &'static Shared {
        // Safety: `_map` is a live mapping sized to `Shared` for this client's
        // lifetime; the returned ref is only used while `self` is alive.
        unsafe { Shared::from_ptr(self.base) }
    }

    /// Spawn a child host that loads `n` instances of the plugin at `bundle`.
    ///
    /// `child_bin` is the path to the `openrig-vst3-proc` executable.
    pub fn spawn(
        child_bin: &Path,
        shm_path: &Path,
        bundle: &Path,
        uid: &[u8; 16],
        sample_rate: f64,
        block_size: usize,
        n: usize,
    ) -> Result<Self> {
        assert!(n >= 1 && n <= MAX_INSTANCES, "n out of range");
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(shm_path)
            .with_context(|| format!("create shm file {}", shm_path.display()))?;
        file.set_len(shared_size() as u64)?;
        // Safety: freshly sized file, we own the mapping for this process.
        let map = unsafe { memmap2::MmapMut::map_mut(&file)? };
        {
            let shared = unsafe { Shared::from_ptr(map.as_ptr() as *mut u8) };
            shared.ready.store(0, Ordering::SeqCst);
            shared.shutdown.store(0, Ordering::SeqCst);
            shared.load_failed.store(0, Ordering::SeqCst);
            shared.n_instances.store(n as u32, Ordering::SeqCst);
            for s in shared.slots.iter().take(n) {
                s.request.store(0, Ordering::SeqCst);
                s.done.store(0, Ordering::SeqCst);
                s.param_write.store(0, Ordering::SeqCst);
            }
        }

        let uid_hex: String = uid.iter().map(|b| format!("{b:02x}")).collect();
        let child = std::process::Command::new(child_bin)
            .arg(CHILD_FLAG)
            .arg(shm_path)
            .arg(bundle)
            .arg(uid_hex)
            .arg(sample_rate.to_string())
            .arg(block_size.to_string())
            .arg(n.to_string())
            .spawn()
            .with_context(|| format!("spawn {}", child_bin.display()))?;

        let base = map.as_ptr() as *mut u8;
        let mut client = Self {
            _file: file,
            _map: map,
            base,
            child,
            n,
        };
        client.wait_ready()?;
        Ok(client)
    }

    fn wait_ready(&mut self) -> Result<()> {
        let shared = self.shared();
        // Loading N plugins can take a moment; wait up to ~10 s.
        for _ in 0..10_000 {
            if shared.load_failed.load(Ordering::SeqCst) != 0 {
                bail!("vst3-proc child failed to load the plugin");
            }
            if shared.ready.load(Ordering::SeqCst) == 1 {
                return Ok(());
            }
            if let Ok(Some(status)) = self.child.try_wait() {
                bail!("vst3-proc child exited before ready: {status}");
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        bail!("vst3-proc child did not become ready in time");
    }

    pub fn instances(&self) -> usize {
        self.n
    }

    /// Ask the child to create `slot`'s instance and wait until it's ready.
    pub fn load_slot(&self, slot: usize) -> Result<()> {
        let s = &self.shared().slots[slot];
        s.load_req.store(1, Ordering::Release);
        for _ in 0..10_000 {
            if s.loaded.load(Ordering::Acquire) == 1 {
                return Ok(());
            }
            if self.shared().load_failed.load(Ordering::SeqCst) != 0 {
                bail!("vst3-proc child failed to load slot {slot}");
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        bail!("vst3-proc slot {slot} did not load in time");
    }

    /// Push a normalized param (0.0..=1.0) to `slot`'s instance.
    pub fn set_param(&self, slot: usize, id: u32, normalized: f32) {
        self.shared().slots[slot].push_param(id, normalized);
    }

    /// Process `frames` through `slot`'s instance in place. Spins until the
    /// child finishes (bounded), leaving the block unchanged on timeout.
    ///
    /// Takes `&self` so one client can be shared across slots (one per stream);
    /// each slot has its own atomics, so distinct slots never contend.
    pub fn process(&self, slot: usize, frames: &mut [[f32; 2]]) {
        let n = frames.len().min(MAX_FRAMES);
        let s = &self.shared().slots[slot];
        for (i, f) in frames.iter().take(n).enumerate() {
            s.write_input(i, *f);
        }
        s.n_frames.store(n as u32, Ordering::SeqCst);
        let target = s.request.fetch_add(1, Ordering::AcqRel) + 1;
        // Bounded spin — the child is a tight loop; a block is sub-millisecond.
        for _ in 0..2_000_000 {
            if s.done.load(Ordering::Acquire) >= target {
                for (i, f) in frames.iter_mut().take(n).enumerate() {
                    *f = s.read_output(i);
                }
                return;
            }
            std::hint::spin_loop();
        }
        // Timed out: leave frames as-is (dry). Better than blocking the caller.
    }
}

impl Drop for Vst3ProcClient {
    fn drop(&mut self) {
        self.shared().shutdown.store(1, Ordering::SeqCst);
        let _ = self.child.wait();
    }
}

/// Parse a 32-hex-char UID string into 16 bytes.
pub fn parse_uid_hex(s: &str) -> Result<[u8; 16]> {
    if s.len() != 32 {
        bail!("uid hex must be 32 chars, got {}", s.len());
    }
    let mut out = [0u8; 16];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).context("bad uid hex")?;
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Child host entry point (runs in the spawned process, no NSApplication)
// ---------------------------------------------------------------------------

/// argv[1] that marks a process launched as an out-of-process host child.
pub const CHILD_FLAG: &str = "--vst3-proc-child";

/// If this process was launched as a host child, run the host loop and exit
/// (returns `true` so the caller stops). Call at the very TOP of the app's
/// `main`, before any NSApplication / GUI / logging init — the child must never
/// touch AppKit (that is the whole point, #251).
pub fn maybe_run_child() -> bool {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) != Some(CHILD_FLAG) {
        return false;
    }
    let code = match run_child_from_args(&args[2..]) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("[vst3-proc] child fatal: {e:#}");
            1
        }
    };
    std::process::exit(code);
}

fn run_child_from_args(args: &[String]) -> Result<()> {
    if args.len() != 6 {
        bail!("child args: <shm> <bundle> <uid_hex> <sr> <block> <n>");
    }
    let uid = parse_uid_hex(&args[2])?;
    run_child(
        Path::new(&args[0]),
        Path::new(&args[1]),
        &uid,
        args[3].parse().context("sample_rate")?,
        args[4].parse().context("block_size")?,
        args[5].parse().context("n")?,
    )
}

/// The host loop: map the shared region, lazily create instances, and service
/// Promote THIS thread to the macOS realtime (time-constraint) class, mirroring
/// the audio `dsp_worker` (#670/#698). The out-of-process host child must be RT:
/// the parent's dsp_worker is RT-promoted and busy-spins in `process()` waiting
/// for us, so if we stay a normal SCHED_OTHER thread the kernel deprioritizes us
/// under load and the RT worker spins on a descheduled child — priority
/// inversion → a flood of underruns (#251 × #760). We can't join the parent's
/// audio workgroup from another process, but RT scheduling alone removes the
/// inversion.
#[cfg(target_os = "macos")]
fn promote_to_audio_rt(period_ns: u64, computation_ns: u64) {
    #[repr(C)]
    struct Timebase {
        numer: u32,
        denom: u32,
    }
    #[repr(C)]
    struct TimeConstraint {
        period: u32,
        computation: u32,
        constraint: u32,
        preemptible: u32,
    }
    extern "C" {
        fn mach_thread_self() -> u32;
        fn mach_timebase_info(info: *mut Timebase) -> i32;
        fn thread_policy_set(thread: u32, flavor: i32, policy: *const u32, count: u32) -> i32;
    }
    const THREAD_TIME_CONSTRAINT_POLICY: i32 = 2;
    unsafe {
        let mut tb = Timebase { numer: 0, denom: 0 };
        if mach_timebase_info(&mut tb) != 0 || tb.numer == 0 {
            return;
        }
        let to_mach = |ns: u64| ((ns as u128 * tb.denom as u128) / tb.numer as u128) as u32;
        let policy = TimeConstraint {
            period: to_mach(period_ns),
            computation: to_mach(computation_ns.min(period_ns * 85 / 100)),
            constraint: to_mach(period_ns),
            preemptible: 1,
        };
        thread_policy_set(
            mach_thread_self(),
            THREAD_TIME_CONSTRAINT_POLICY,
            &policy as *const _ as *const u32,
            4,
        );
    }
}

#[cfg(not(target_os = "macos"))]
fn promote_to_audio_rt(_period_ns: u64, _computation_ns: u64) {}

/// each slot's request/done handshake and param ring. Blocks until shutdown.
pub fn run_child(
    shm_path: &Path,
    bundle: &Path,
    uid: &[u8; 16],
    sample_rate: f64,
    block_size: usize,
    n: usize,
) -> Result<()> {
    use block_core::StereoProcessor;
    use vst3_host::{StereoVst3Processor, Vst3Plugin};

    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(shm_path)
        .with_context(|| format!("open shm {}", shm_path.display()))?;
    // Safety: the parent sized this file to `Shared` before spawning us.
    let map = unsafe { memmap2::MmapMut::map_mut(&file)? };
    let shared: &Shared = unsafe { Shared::from_ptr(map.as_ptr() as *mut u8) };

    let cap = n.min(MAX_INSTANCES);
    let mut procs: Vec<Option<StereoVst3Processor>> = (0..cap).map(|_| None).collect();

    // Match the audio callback's cadence so the kernel schedules us in lockstep
    // with the RT worker that spins on us (period = block/sr).
    let period_ns = ((block_size as f64 / sample_rate) * 1e9) as u64;
    promote_to_audio_rt(period_ns, period_ns * 85 / 100);

    shared.ready.store(1, Ordering::SeqCst);

    let mut last_done = vec![0u64; cap];
    let mut last_param = vec![0u64; cap];
    let mut scratch: Vec<[f32; 2]> = vec![[0.0; 2]; MAX_FRAMES];
    let mut idle_polls: u32 = 0;

    loop {
        if shared.shutdown.load(Ordering::SeqCst) != 0 {
            return Ok(());
        }
        // Exit if the parent process is gone (orphaned → reparented to init on
        // unix), so a child never lingers after the app closes.
        #[cfg(unix)]
        if unsafe { libc::getppid() } == 1 {
            return Ok(());
        }
        let mut did_work = false;
        for i in 0..cap {
            let slot = &shared.slots[i];
            if slot.load_req.load(Ordering::Acquire) == 1 && procs[i].is_none() {
                match Vst3Plugin::load(bundle, uid, sample_rate, 2, block_size, &[]) {
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
            let Some(proc) = procs[i].as_mut() else { continue };

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
                proc.process_block(&mut scratch[..frames]);
                for f in 0..frames {
                    slot.write_output(f, scratch[f]);
                }
                slot.done.store(req, Ordering::Release);
                last_done[i] = req;
            }
        }
        if did_work {
            idle_polls = 0;
        } else {
            // Spin briefly for low latency when audio is flowing, then back off
            // to a short sleep so an idle host doesn't peg a CPU core.
            idle_polls = idle_polls.saturating_add(1);
            if idle_polls < 2_000 {
                std::hint::spin_loop();
            } else {
                std::thread::sleep(std::time::Duration::from_micros(200));
            }
        }
    }
}

/// Find the installed standalone `openrig-vst3-proc` next to the app executable,
/// a couple of directories up (so `target/debug/deps/<test>` finds
/// `target/debug/openrig-vst3-proc`), or in any `target/{debug,release}` walking
/// up. Returns `None` if it isn't there (the caller falls back to self-exec).
pub fn find_installed_child_bin() -> Option<PathBuf> {
    const BIN: &str = "openrig-vst3-proc";
    let exe = std::env::current_exe().ok()?;
    let mut candidates: Vec<PathBuf> = Vec::new();
    let mut dir = exe.parent().map(|p| p.to_path_buf());
    for _ in 0..3 {
        if let Some(d) = &dir {
            candidates.push(d.join(BIN));
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }
    let mut up = exe.parent().map(|p| p.to_path_buf());
    for _ in 0..8 {
        if let Some(d) = &up {
            candidates.push(d.join("target/debug").join(BIN));
            candidates.push(d.join("target/release").join(BIN));
            up = d.parent().map(|p| p.to_path_buf());
        }
    }
    candidates.into_iter().find(|c| c.exists())
}

// ---------------------------------------------------------------------------
// Global manager — one child host per (bundle, uid), slots handed out per stream
// ---------------------------------------------------------------------------

// Safety: cross-slot access to the shared mapping is serialised by per-slot
// atomics; distinct slots never touch the same memory, so a shared client can be
// used from many threads (one stream each).
unsafe impl Send for Vst3ProcClient {}
unsafe impl Sync for Vst3ProcClient {}

use std::sync::Mutex;

static CHILD_BIN: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Set the path to the `openrig-vst3-proc` executable (the app calls this once
/// at startup; it ships next to the app binary).
pub fn set_child_bin(path: PathBuf) {
    *CHILD_BIN.lock().unwrap() = Some(path);
}

fn child_bin() -> Result<PathBuf> {
    if let Some(p) = CHILD_BIN.lock().unwrap().clone() {
        return Ok(p);
    }
    // The child MUST be the lean standalone `openrig-vst3-proc`. Re-executing the
    // GUI app is not viable: its dylibs bring up NSApplication at load, which is
    // exactly what breaks JUCE `createInstance` (#251) — verified to SIGABRT.
    find_installed_child_bin().ok_or_else(|| {
        anyhow::anyhow!(
            "out-of-process VST3 host 'openrig-vst3-proc' not found next to the app; \
             build it with `cargo build --release --bin openrig-vst3-proc` (or bundle \
             it beside the app binary)"
        )
    })
}

static SHM_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A single out-of-process VST3 instance, backed by its OWN dedicated child
/// process (one plugin per child). Stream isolation (invariant #4): no shm,
/// lock, or host loop is shared between two instances. It also sidesteps the
/// JUCE multi-instance limitation — each child only ever performs
/// `createInstance` #1, which is the one that reliably succeeds (#251).
pub struct ProcHandle {
    client: Vst3ProcClient,
}

impl ProcHandle {
    pub fn process_block(&self, frames: &mut [[f32; 2]]) {
        self.client.process(0, frames);
    }
    pub fn set_param(&self, id: u32, normalized: f32) {
        self.client.set_param(0, id, normalized);
    }
}

// ---------------------------------------------------------------------------
// Processor proxies — a StereoProcessor / MonoProcessor backed by a ProcHandle
// ---------------------------------------------------------------------------

use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor, MonoProcessor, StereoProcessor};

fn apply_param_set(handle: &ProcHandle, params: &ParameterSet) {
    for (path, value) in params.values.iter() {
        let Some(id) = path.strip_prefix('p').and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let Some(pct) = value.as_f32() else { continue };
        handle.set_param(id, (pct / 100.0).clamp(0.0, 1.0));
    }
}

/// Stereo audio processor whose DSP runs in the out-of-process host.
pub struct ProcStereoProcessor {
    handle: ProcHandle,
}
impl StereoProcessor for ProcStereoProcessor {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let mut b = [input];
        self.handle.process_block(&mut b);
        b[0]
    }
    fn process_block(&mut self, frames: &mut [[f32; 2]]) {
        self.handle.process_block(frames);
    }
    fn try_in_place_update(&mut self, params: &ParameterSet, _sr: f32) -> bool {
        apply_param_set(&self.handle, params);
        true
    }
}

/// Mono adapter: feeds the stereo out-of-process instance a duplicated mono
/// signal and takes the left channel back.
pub struct ProcMonoProcessor {
    handle: ProcHandle,
    buf: Vec<[f32; 2]>,
}
impl MonoProcessor for ProcMonoProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let mut b = [[input, input]];
        self.handle.process_block(&mut b);
        b[0][0]
    }
    fn process_block(&mut self, buffer: &mut [f32]) {
        self.buf.clear();
        self.buf.extend(buffer.iter().map(|&s| [s, s]));
        self.handle.process_block(&mut self.buf);
        for (o, w) in buffer.iter_mut().zip(self.buf.iter()) {
            *o = w[0];
        }
    }
    fn try_in_place_update(&mut self, params: &ParameterSet, _sr: f32) -> bool {
        apply_param_set(&self.handle, params);
        true
    }
}

/// Build an out-of-process VST3 processor for `layout`, applying `initial_params`
/// (normalized `(id, value)` pairs). Used by the engine in place of the
/// in-process `Vst3Plugin` build for GUI plugins (#251).
pub fn build_vst3_proc_processor(
    bundle: &Path,
    uid: &[u8; 16],
    sample_rate: f64,
    block_size: usize,
    layout: AudioChannelLayout,
    initial_params: &[(u32, f64)],
) -> Result<BlockProcessor> {
    let handle = acquire(bundle, uid, sample_rate, block_size)?;
    for &(id, norm) in initial_params {
        handle.set_param(id, norm as f32);
    }
    Ok(match layout {
        AudioChannelLayout::Mono => {
            BlockProcessor::Mono(Box::new(ProcMonoProcessor { handle, buf: Vec::new() }))
        }
        AudioChannelLayout::Stereo => BlockProcessor::Stereo(Box::new(ProcStereoProcessor { handle })),
    })
}

/// Acquire an out-of-process instance of the plugin at `bundle`, in its OWN
/// dedicated child process (one plugin per child — see [`ProcHandle`]). Every
/// call spawns a fresh, fully isolated child and loads the single instance.
pub fn acquire(
    bundle: &Path,
    uid: &[u8; 16],
    sample_rate: f64,
    block_size: usize,
) -> Result<ProcHandle> {
    let id = SHM_COUNTER.fetch_add(1, Ordering::SeqCst);
    let shm =
        std::env::temp_dir().join(format!("openrig-vst3-{}-{}.shm", std::process::id(), id));
    let client = Vst3ProcClient::spawn(&child_bin()?, &shm, bundle, uid, sample_rate, block_size, 1)?;
    client.load_slot(0)?;
    Ok(ProcHandle { client })
}
