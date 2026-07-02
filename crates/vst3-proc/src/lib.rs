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

/// Best-effort path to the built child binary. Looks next to the current
/// executable and one directory up (so `target/debug/deps/<test>` finds
/// `target/debug/openrig-vst3-proc`).
pub fn default_child_bin() -> PathBuf {
    const BIN: &str = "openrig-vst3-proc";
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let here = dir.join(BIN);
            if here.exists() {
                return here;
            }
            if let Some(up) = dir.parent() {
                let up_bin = up.join(BIN);
                if up_bin.exists() {
                    return up_bin;
                }
            }
        }
    }
    PathBuf::from(BIN)
}

// ---------------------------------------------------------------------------
// Global manager — one child host per (bundle, uid), slots handed out per stream
// ---------------------------------------------------------------------------

// Safety: cross-slot access to the shared mapping is serialised by per-slot
// atomics; distinct slots never touch the same memory, so a shared client can be
// used from many threads (one stream each).
unsafe impl Send for Vst3ProcClient {}
unsafe impl Sync for Vst3ProcClient {}

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

static CHILD_BIN: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Set the path to the `openrig-vst3-proc` executable (the app calls this once
/// at startup; it ships next to the app binary).
pub fn set_child_bin(path: PathBuf) {
    *CHILD_BIN.lock().unwrap() = Some(path);
}

fn child_bin() -> PathBuf {
    CHILD_BIN
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(default_child_bin)
}

struct Host {
    client: Vst3ProcClient,
    next_slot: std::sync::atomic::AtomicUsize,
}

type HostKey = (PathBuf, [u8; 16]);
static HOSTS: OnceLock<Mutex<HashMap<HostKey, Arc<Host>>>> = OnceLock::new();

fn hosts() -> &'static Mutex<HashMap<HostKey, Arc<Host>>> {
    HOSTS.get_or_init(|| Mutex::new(HashMap::new()))
}

static SHM_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A single out-of-process VST3 instance (one slot of a shared child host).
pub struct ProcHandle {
    host: Arc<Host>,
    slot: usize,
}

impl ProcHandle {
    pub fn process_block(&self, frames: &mut [[f32; 2]]) {
        self.host.client.process(self.slot, frames);
    }
    pub fn set_param(&self, id: u32, normalized: f32) {
        self.host.client.set_param(self.slot, id, normalized);
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

/// Acquire an out-of-process instance of the plugin at `bundle`. Reuses the
/// per-(bundle,uid) child host, allocating one more slot.
pub fn acquire(
    bundle: &Path,
    uid: &[u8; 16],
    sample_rate: f64,
    block_size: usize,
) -> Result<ProcHandle> {
    let key: HostKey = (bundle.to_path_buf(), *uid);
    let host = {
        let mut map = hosts().lock().unwrap();
        if let Some(h) = map.get(&key) {
            h.clone()
        } else {
            let id = SHM_COUNTER.fetch_add(1, Ordering::SeqCst);
            let shm = std::env::temp_dir()
                .join(format!("openrig-vst3-{}-{}.shm", std::process::id(), id));
            let client = Vst3ProcClient::spawn(
                &child_bin(),
                &shm,
                bundle,
                uid,
                sample_rate,
                block_size,
                MAX_INSTANCES,
            )?;
            let h = Arc::new(Host {
                client,
                next_slot: std::sync::atomic::AtomicUsize::new(0),
            });
            map.insert(key, h.clone());
            h
        }
    };
    let slot = host.next_slot.fetch_add(1, Ordering::SeqCst);
    if slot >= MAX_INSTANCES {
        bail!("out-of-process VST3 host is full ({MAX_INSTANCES} instances)");
    }
    host.client.load_slot(slot)?;
    Ok(ProcHandle { host, slot })
}
