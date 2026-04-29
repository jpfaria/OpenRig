//! Crate-wide default and supported audio settings, plus the magic
//! prefixes used by the Select-block parameter pathing.

pub(crate) const DEFAULT_SAMPLE_RATE: u32 = 48_000;
pub(crate) const DEFAULT_BUFFER_SIZE_FRAMES: u32 = 64;
pub(crate) const DEFAULT_BIT_DEPTH: u32 = 32;
pub(crate) const SUPPORTED_SAMPLE_RATES: &[u32] = &[44_100, 48_000, 88_200, 96_000];
pub(crate) const SUPPORTED_BUFFER_SIZES: &[u32] = &[32, 64, 128, 256, 512, 1024];
pub(crate) const SUPPORTED_BIT_DEPTHS: &[u32] = &[16, 24, 32];

pub(crate) const SELECT_PATH_PREFIX: &str = "__select.";
pub(crate) const SELECT_SELECTED_BLOCK_ID: &str = "__select.selected_block_id";
