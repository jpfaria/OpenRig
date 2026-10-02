//! Responsibility: routes the drum content loaders.

mod drum_library;
mod groove_file;
mod hydrogen_kit;
mod kit_loader;
mod kit_roles;

pub use drum_library::{bundled_drum_dir, scan_drum_library, DrumKitEntry, DrumLibrary};
pub use groove_file::parse_groove_file;
pub use hydrogen_kit::{parse_hydrogen_kit, KitDescription, KitInstrument, KitLayerFiles};
pub use kit_loader::load_kit;
pub use kit_roles::{assign_roles, role_for_name};
