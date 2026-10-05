//! Responsibility: refuses to switch on a chain whose interface is absent.
//!
//! A chain whose interface is absent would look on while no stream opens, so
//! it does not switch on, and the refusal names the missing interface.

use anyhow::{anyhow, Result};
use project::chain::Chain;

use crate::chain_missing_devices::chain_missing_devices;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// `Err` naming every absent interface `candidate` uses. No presence
    /// source attached ⇒ nothing to ask, `Ok`.
    pub(crate) fn ensure_chain_devices_present(&self, candidate: &Chain) -> Result<()> {
        let Some(presence) = self.device_presence.borrow().clone() else {
            return Ok(());
        };
        let missing =
            chain_missing_devices(candidate, &self.io_binding_registry(), presence.as_ref());
        if missing.is_empty() {
            return Ok(());
        }
        Err(anyhow!(
            "chain '{}' cannot be enabled: audio interface not available ({}) — turn it on or plug it in",
            candidate.id.0,
            missing.join(", ")
        ))
    }
}
