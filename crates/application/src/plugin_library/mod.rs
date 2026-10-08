//! Responsibility: routes the plugin library modules.
//!
//! The plugin library is every plugin the user owns: the ones in the
//! plugins folder and the TONE3000 installs. The plugins the app ships are
//! never listed or changed.

pub mod block_follow;
pub mod capture_names;
pub mod create;
pub mod disk_manifest;
pub mod entries;
pub mod grid_apply;
pub mod grid_read;
pub mod grid_types;
pub mod manifest_grid;
pub mod manifest_save;
pub mod redo_worker;
pub mod reinfer;
pub mod roots;

pub use grid_types::{CaptureBackend, ColumnKind, EditorGrid, GridColumn, GridRow};
pub use roots::{PluginOrigin, PluginRoots};
