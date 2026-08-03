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

/// #127/#323: the LOOPER half of the GUI's `RuntimeControl`.
///
/// Unlike `DiRuntimeControl` above, this does not restate the GUI's bodies —
/// it calls them (`adapter_gui::runtime_loopers`), so a test here proves the
/// real door and not a copy of it. The one thing it leaves out is the #808
/// wake (`ensure_runtime`), which is `GuiRuntimeControl`'s and needs a live
/// `ProjectSession`; these tests hand it a controller that already exists, so
/// there is nothing to wake. The wake itself is covered in-crate.
pub struct LooperRuntimeControl {
    pub runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
}

impl LooperRuntimeControl {
    pub fn attach(
        dispatcher: &application::local_dispatcher::LocalDispatcher,
        controller: ProjectRuntimeController,
    ) -> Rc<RefCell<Option<ProjectRuntimeController>>> {
        let runtime = Rc::new(RefCell::new(Some(controller)));
        dispatcher.attach_runtime_control(Rc::new(LooperRuntimeControl {
            runtime: Rc::clone(&runtime),
        }));
        runtime
    }
}

impl RuntimeControl for LooperRuntimeControl {
    fn create_looper(&self, chain: &Chain, looper: u64) -> anyhow::Result<()> {
        adapter_gui::runtime_loopers::create(&self.runtime, chain, looper);
        Ok(())
    }

    fn remove_looper(&self, chain: &Chain, looper: u64) {
        adapter_gui::runtime_loopers::remove(&self.runtime, chain, looper);
    }

    fn looper_transport(
        &self,
        chain: &Chain,
        looper: u64,
        action: application::command::LooperAction,
    ) -> anyhow::Result<()> {
        adapter_gui::runtime_loopers::transport(&self.runtime, chain, looper, action);
        Ok(())
    }

    fn set_looper_param(
        &self,
        chain: &Chain,
        looper: u64,
        param: application::command::LooperParam,
    ) {
        adapter_gui::runtime_loopers::set_param(&self.runtime, chain, looper, param);
    }

    fn set_looper_input(
        &self,
        chain: &Chain,
        looper: u64,
        input: Option<project::chain::EndpointRef>,
    ) {
        adapter_gui::runtime_loopers::set_input(&self.runtime, chain, looper, input);
    }

    fn set_looper_output(
        &self,
        chain: &Chain,
        looper: u64,
        output: Option<project::chain::EndpointRef>,
    ) {
        adapter_gui::runtime_loopers::set_output(&self.runtime, chain, looper, output);
    }

    fn export_chain_loops(&self, chain: &Chain) -> Option<Vec<(u64, Arc<engine::LoopPcm>)>> {
        adapter_gui::runtime_loopers::export_chain_loops(&self.runtime, chain)
    }
}
