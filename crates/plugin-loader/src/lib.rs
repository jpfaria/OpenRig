//! Runtime loader for OpenRig plugin packages.
//!
//! Reads `.openrig-plugin` packages (NAM / IR / LV2) from a directory and
//! registers them in the runtime catalog. Replaces the compile-time
//! `MODEL_DEFINITION` codegen path.
//!
//! Issue: #287

pub mod manifest;
pub mod validate;

pub use manifest::{Backend, BlockType, GridCapture, GridParameter, Lv2Slot, PluginManifest};
pub use validate::{validate_manifest, ValidationError, MAX_SUPPORTED_VERSION};
