//! Responsibility: finishes or drops a TONE3000 download that waits for parameter names.
//!
//! Both run on the dispatching thread: a finish rewrites one small manifest
//! and moves a folder, a cancel removes one.

use anyhow::{anyhow, bail, Result};

use crate::event::{Event, Tone3000Event};
use crate::local_dispatcher::LocalDispatcher;
use crate::plugin_library::EditorGrid;
use crate::tone3000::install_pending::{drop_pending, finish};

impl LocalDispatcher {
    /// Installs the waiting tone with the parameters `grid` names. A grid
    /// that fails keeps the tone waiting, so the user can fix it.
    pub(crate) fn finish_tone3000_install(
        &self,
        tone_id: u64,
        grid: &EditorGrid,
    ) -> Result<Vec<Event>> {
        let state = self.tone3000.borrow().clone();
        let Some(pending) = state.borrow().pending(tone_id).cloned() else {
            bail!("TONE3000 tone {tone_id} is not waiting for names");
        };
        let installed = finish(&pending, grid)?;
        state.borrow_mut().named(tone_id);
        state.borrow_mut().refresh_installed();
        plugin_loader::registry::load_one(&installed.plugin_id, &[installed.dir])
            .map_err(|e| anyhow!("installed, but the catalog did not load it: {e}"))?;
        Ok(vec![Event::Tone3000(Tone3000Event::Installed {
            tone_id,
            plugin_id: installed.plugin_id,
        })])
    }

    pub(crate) fn cancel_tone3000_install(&self, tone_id: u64) -> Result<Vec<Event>> {
        let state = self.tone3000.borrow().clone();
        let Some(pending) = state.borrow().pending(tone_id).cloned() else {
            bail!("TONE3000 tone {tone_id} is not waiting for names");
        };
        drop_pending(&pending)?;
        state.borrow_mut().named(tone_id);
        Ok(vec![Event::Tone3000(Tone3000Event::InstallCanceled {
            tone_id,
        })])
    }
}
