//! Responsibility: keeps the historical `rig_methods` path alive for importers.
//!
//! The methods themselves moved into `rig_write_back.rs`, `rig_nav.rs` and
//! `rig_validate.rs` (#873); an inherent method needs no import, so this file
//! only carries what used to hang off it.
