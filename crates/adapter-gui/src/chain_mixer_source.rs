//! Responsibility: reads what one chain's mixer shows out of the project session.
//! #1007 — the compact view's mixer section and the chain list's mixer
//! overlay draw the same thing: every strip (a solo elsewhere dims ours), the
//! chain's strip ids and the chain itself (its own faders, loopers and
//! volume). Both poll the state and redraw only when it changes.

use std::cell::RefCell;
use std::rc::Rc;

use application::chain_fader_view::chain_fader_views;
use application::chain_mixer_strips::chain_mixer_strip_ids;
use application::mixer_view::MixerStripView;
use domain::ids::ChainId;
use project::chain::Chain;

use crate::chain_mixer_rows::{chain_mixer_rows, ChainMixerRows};
use crate::mixer_rows::mixer_rows_of;
use crate::state::ProjectSession;
use crate::MixerStripRow;

type Session = Rc<RefCell<Option<ProjectSession>>>;

/// What a chain's mixer is drawn from: every strip, the chain's strip ids and
/// the chain.
pub(crate) type Drawn = (Vec<MixerStripView>, Vec<String>, Option<Chain>);

/// What a chain's mixer puts on screen: the global strips of the chain's
/// endpoints (inputs, outputs) and, while the chain exists, its own faders.
pub(crate) struct ChainMixerView {
    pub inputs: Vec<MixerStripRow>,
    pub outputs: Vec<MixerStripRow>,
    pub chain: Option<ChainMixerRows>,
}

pub(crate) fn chain_mixer_state(session: &Session, chain_index: usize) -> Drawn {
    let borrowed = session.borrow();
    let Some(session) = borrowed.as_ref() else {
        return (Vec::new(), Vec::new(), None);
    };
    let chain = session.project.borrow().chains.get(chain_index).cloned();
    let ids = chain
        .as_ref()
        .map(|chain| chain_mixer_strip_ids(chain, &session.io_bindings.borrow()))
        .unwrap_or_default();
    (session.dispatcher.mixer_strips(), ids, chain)
}

pub(crate) fn chain_mixer_view(session: &Session, drawn: &Drawn) -> ChainMixerView {
    let (inputs, outputs) = mixer_rows_of(&drawn.0, &drawn.1);
    let chain = drawn.2.as_ref().map(|chain| {
        let views = session
            .borrow()
            .as_ref()
            .map(|session| chain_fader_views(chain, &session.io_bindings.borrow()))
            .unwrap_or_default();
        chain_mixer_rows(&inputs, &outputs, &views, chain)
    });
    ChainMixerView {
        inputs,
        outputs,
        chain,
    }
}

pub(crate) fn chain_id_at(session: &Session, chain_index: usize) -> Option<ChainId> {
    let borrowed = session.borrow();
    let session = borrowed.as_ref()?;
    let project = session.project.borrow();
    project.chains.get(chain_index).map(|c| c.id.clone())
}
