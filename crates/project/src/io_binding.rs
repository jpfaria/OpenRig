//! I/O binding types — re-exported from `domain` (single source of truth).
//!
//! All three consumers (`domain`, `project`, `infra-filesystem`) share one
//! canonical definition. This module exists so the path
//! `project::io_binding::IoBinding` continues to resolve for code that was
//! already using it.

pub use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
