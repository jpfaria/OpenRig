//! Runtime loader for OpenRig plugin packages.
//!
//! Reads `.openrig-plugin` packages (NAM / IR / LV2) from a directory and
//! registers them in the runtime catalog. Replaces the compile-time
//! `MODEL_DEFINITION` codegen path.
//!
//! Issue: #287

pub mod config;
pub mod discover;
pub mod dispatch;
pub mod manifest;
pub mod native_runtimes;
pub mod package;
pub mod package_builders;
pub mod registry;
pub mod validate;

pub use config::{plugins_root_from_config, PluginPathsConfig};
pub use discover::{discover, DiscoveryError, LoadedPackage};
pub use manifest::{Backend, BlockType, GridCapture, GridParameter, Lv2Slot, PluginManifest};
pub use native_runtimes::{NativeBuildFn, NativeRuntime, NativeSchemaFn, NativeValidateFn};
pub use package::{current_platform_slot, validate_package, PackageError};
pub use validate::{validate_manifest, ValidationError, MAX_SUPPORTED_VERSION};
