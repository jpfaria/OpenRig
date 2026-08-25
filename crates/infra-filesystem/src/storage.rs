//! Responsibility: names the facade every filesystem-backed accessor hangs off.
//!
//! An empty type on purpose: the accessors are grouped into one namespace so a
//! caller reads `FilesystemStorage::load_app_config()` rather than importing a
//! loose function, and each group of them lives in its own file.

pub struct FilesystemStorage;
