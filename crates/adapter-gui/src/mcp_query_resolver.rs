//! Frontend side of the read bus: hands the GUI thread's live state
//! (`!Send` project, runtime meters, analyzer sessions) to the one
//! [`QueryKind`] resolver.
//!
//! #831: this file used to carry its own copy of the `QueryKind` match and
//! decide, per read, what the GUI answers — which is how the payloads
//! drifted between transports. It now only borrows what a read needs and
//! calls [`application::read::resolve`]; the single match lives in
//! `application::read`, and everything live comes from [`GuiLiveSource`].

use std::cell::RefCell;
use std::rc::Rc;

use application::bridge::QueryKind;
use application::read::{resolve, ReadContext};
use slint::VecModel;

use infra_cpal::ProjectRuntimeController;

use crate::gui_live_source::GuiLiveSource;
use crate::spectrum_session::SpectrumSession;
use crate::state::ProjectSession;
use crate::tuner_session::TunerSession;
use crate::ProjectChainItem;

/// Live handles the resolver reads from. Borrowed per tick — nothing is
/// cached, so a reply always reflects the frame the user is looking at.
pub(crate) struct QueryResolver<'a> {
    pub(crate) session: &'a ProjectSession,
    /// Chain rows the GUI meters write into (`meter_in_dbfs` / `meter_out_dbfs`).
    pub(crate) chain_rows: &'a Rc<VecModel<ProjectChainItem>>,
    pub(crate) tuner: &'a Rc<RefCell<Option<TunerSession>>>,
    pub(crate) spectrum: &'a Rc<RefCell<Option<SpectrumSession>>>,
    /// Live runtime — DI playback state and peaks come from it, per chain.
    pub(crate) runtime: &'a Rc<RefCell<Option<ProjectRuntimeController>>>,
}

impl QueryResolver<'_> {
    pub(crate) fn resolve(&self, kind: &QueryKind) -> Result<String, String> {
        // Every borrow is held for exactly this call — the read path never
        // mutates, so the project stays borrowed while the resolver reads
        // it and the live source reads the rows aligned with it.
        let project = self.session.project.borrow();
        let rig = self.session.rig.as_ref().map(|rig| rig.borrow());
        let io_bindings = self.session.io_bindings.borrow();
        let live = GuiLiveSource {
            project: &project,
            chain_rows: self.chain_rows,
            io_bindings: &io_bindings,
            tuner: self.tuner,
            spectrum: self.spectrum,
            runtime: self.runtime,
        };
        resolve(
            kind,
            &ReadContext {
                project: &project,
                rig: rig.as_deref(),
                io_bindings: &io_bindings,
                dispatcher: self.session.dispatcher.as_ref(),
                live: &live,
            },
        )
    }
}

#[cfg(test)]
#[path = "mcp_query_resolver_tests.rs"]
mod tests;
