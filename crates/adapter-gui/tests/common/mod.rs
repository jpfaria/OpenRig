//! #127: the DI half of the GUI's `RuntimeControl`, for integration tests.
//!
//! Arming a chain's DI is applied by the DISPATCHER now, through
//! `application::runtime_control::RuntimeControl`. The GUI's own
//! implementation (`GuiRuntimeControl`, in `runtime_lifecycle`) is crate-private
//! and needs a live `ProjectSession`; it is covered by the in-crate tests in
//! `src/runtime_lifecycle_control_tests.rs`, which drive the real thing.
//!
//! What the tests here need is the OTHER half of the same claim: that a
//! command reaches a real `ProjectRuntimeController` and arms it. So this
//! control forwards to a controller built with `for_testing` (no devices
//! opened), with the same body the GUI's implementation has.

#![allow(dead_code)] // each integration-test binary uses a subset

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use application::runtime_control::RuntimeControl;
use domain::ids::ChainId;
use engine::DiPcm;
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;

pub struct DiRuntimeControl {
    pub runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
}

impl DiRuntimeControl {
    /// Attach a control over `runtime` to `dispatcher`, and hand the runtime
    /// handle back so the test can assert on it.
    pub fn attach(
        dispatcher: &application::local_dispatcher::LocalDispatcher,
        controller: ProjectRuntimeController,
    ) -> Rc<RefCell<Option<ProjectRuntimeController>>> {
        let runtime = Rc::new(RefCell::new(Some(controller)));
        dispatcher.attach_runtime_control(Rc::new(DiRuntimeControl {
            runtime: Rc::clone(&runtime),
        }));
        runtime
    }
}

impl RuntimeControl for DiRuntimeControl {
    fn arm_di_stream(&self, chain: &Chain, pcm: Arc<DiPcm>) -> anyhow::Result<()> {
        let borrow = self.runtime.borrow();
        let Some(runtime) = borrow.as_ref() else {
            return Ok(());
        };
        runtime.arm_di_stream(chain, pcm)
    }

    fn disarm_di_stream(&self, chain: &ChainId) {
        if let Some(runtime) = self.runtime.borrow().as_ref() {
            runtime.disarm_di_stream(chain);
        }
    }

    fn refresh_di_stream(&self, chain: &Chain, pcm: Arc<DiPcm>) -> anyhow::Result<()> {
        let borrow = self.runtime.borrow();
        let Some(runtime) = borrow.as_ref() else {
            return Ok(());
        };
        if !runtime.di_stream_active(&chain.id) {
            return Ok(());
        }
        runtime.arm_di_stream(chain, pcm)
    }
}
