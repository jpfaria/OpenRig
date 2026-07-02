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

/// One plugin instance's mailbox: audio in/out + a request/done handshake and a
/// single-slot parameter update. Interleaved stereo `[L, R]` frames.
#[repr(C)]
pub struct Slot {
    /// Parent bumps this to request processing of `n_frames`.
    pub request: AtomicU64,
    /// Child sets this equal to `request` once the block is processed.
    pub done: AtomicU64,
    pub n_frames: AtomicU32,
    /// Child sets 1 once this slot's plugin instance is created.
    pub loaded: AtomicU32,
    /// Parent bumps when it writes a new param into `param_id`/`param_bits`.
    pub param_seq: AtomicU64,
    pub param_id: AtomicU32,
    /// f32 normalized value, bit-cast.
    pub param_bits: AtomicU32,
    /// Audio buffers. Written/read by whichever side owns the block per the
    /// request/done handshake, so `UnsafeCell` interior mutability is sound.
    input: [UnsafeCell<[f32; 2]>; MAX_FRAMES],
    output: [UnsafeCell<[f32; 2]>; MAX_FRAMES],
}

impl Slot {
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
    /// Monotonic per-slot request counters (parent-owned).
    req: Vec<u64>,
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
                s.param_seq.store(0, Ordering::SeqCst);
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
            req: vec![0; n],
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

    /// Push a normalized param (0.0..=1.0) to `slot`'s instance.
    pub fn set_param(&mut self, slot: usize, id: u32, normalized: f32) {
        let s = &self.shared().slots[slot];
        s.param_id.store(id, Ordering::SeqCst);
        s.param_bits.store(normalized.to_bits(), Ordering::SeqCst);
        s.param_seq.fetch_add(1, Ordering::SeqCst);
    }

    /// Process `frames` through `slot`'s instance in place. Spins until the
    /// child finishes (bounded), leaving the block unchanged on timeout.
    pub fn process(&mut self, slot: usize, frames: &mut [[f32; 2]]) {
        let n = frames.len().min(MAX_FRAMES);
        let s = &self.shared().slots[slot];
        for (i, f) in frames.iter().take(n).enumerate() {
            s.write_input(i, *f);
        }
        s.n_frames.store(n as u32, Ordering::SeqCst);
        self.req[slot] += 1;
        let target = self.req[slot];
        s.request.store(target, Ordering::Release);
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

/// Best-effort path to the built child binary next to the current executable.
pub fn default_child_bin() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("openrig-vst3-proc")))
        .unwrap_or_else(|| PathBuf::from("openrig-vst3-proc"))
}
