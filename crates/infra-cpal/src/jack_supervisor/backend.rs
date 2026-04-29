//! `JackBackend` trait — the single seam between the supervisor's state
//! machine and the real `jackd` process + `libjack` library. Tests substitute
//! a `MockBackend` so the state machine can be exercised without starting any
//! real processes or connecting to any real servers.
//!
//! The real backend lives in `live_backend.rs` (phase 2). All logic about
//! transitions, ordering, retries and event emission stays in the supervisor.
//!
//! `dead_code` is allowed at the file level because `MockBackend` +
//! `MockCall` only have consumers in `#[cfg(test)]`; on a production Linux
//! release build the trait is only observed through its concrete
//! `LiveJackBackend` implementation, which uses concrete dispatch.

#![allow(dead_code)]

use anyhow::Result;
use std::sync::{Arc, Mutex};

use super::types::{JackConfig, JackMeta, ServerName};

/// Non-destructive verdict the supervisor uses after `spawn` reports "socket
/// appeared" to confirm jackd didn't exit immediately (e.g. ALSA "Broken
/// pipe" on too-small buffer). The supervisor only transitions to `Ready`
/// when this returns `Healthy`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PostReadyStatus {
    Healthy,
    SocketVanished,
    DriverFailure(String),
}

/// Abstraction over the operations the supervisor needs to control jackd.
/// Implementations own all backend-specific state (PIDs, `Child` handles,
/// reaper threads, libjack client connections). The supervisor never touches
/// any of that directly.
///
/// All methods that mutate backend state take `&mut self`; the supervisor
/// owns its backend exclusively so external locking isn't required.
pub trait JackBackend: Send + 'static {
    /// Spawn a `jackd` process for `name` with `config`, wait for its UNIX
    /// socket to appear, then block for the post-ready settling window. On
    /// success the server should already be reachable by libjack clients.
    /// Failure means either the process didn't start or the socket never
    /// appeared within the backend's own timeout.
    fn spawn(&mut self, name: &ServerName, config: &JackConfig) -> Result<()>;

    /// Send SIGTERM to the tracked jackd for `name`, wait for exit (up to
    /// backend-defined timeout), and SIGKILL if it refuses to die. After this
    /// returns `Ok`, the socket must be gone.
    fn terminate(&mut self, name: &ServerName) -> Result<()>;

    /// Probe the server for its metadata (sample_rate, buffer_size, ports,
    /// hw_name). The backend is expected to retry internally to survive
    /// shm-init race windows; on persistent failure this returns `Err`.
    fn probe_meta(&mut self, name: &ServerName) -> Result<JackMeta>;

    /// Cheap filesystem check — true iff the jackd socket is present.
    fn is_socket_present(&self, name: &ServerName) -> bool;

    /// Checks the state of a server *immediately after* `spawn` reported
    /// ready — used by the supervisor to confirm jackd didn't exit during
    /// the ALSA init settling window. Expected to be cheap (no client open).
    fn post_ready_status(&mut self, name: &ServerName) -> PostReadyStatus;

    /// Drop all backend-owned resources for `name` — stops reapers, clears
    /// cached meta, forgets PIDs. Called when the supervisor transitions out
    /// of any non-terminal state.
    fn forget(&mut self, name: &ServerName);
}

/// Scripted mock backend used in supervisor unit tests. Each operation is
/// driven by queued responses configured before the test runs; every call is
/// recorded in `events` so assertions can verify invariants like "forget ran
/// before the next spawn" or "terminate was never called on a server with
/// active clients".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MockCall {
    Spawn(ServerName, JackConfig),
    Terminate(ServerName),
    ProbeMeta(ServerName),
    PostReadyStatus(ServerName),
    Forget(ServerName),
}

#[derive(Default)]
pub struct MockBackendInner {
    pub calls: Vec<MockCall>,
    pub running: std::collections::HashSet<ServerName>,
    pub meta_for: std::collections::HashMap<ServerName, JackMeta>,
    /// Queue of spawn outcomes. Pop one per spawn call; if empty, success.
    pub spawn_script: std::collections::VecDeque<std::result::Result<(), String>>,
    /// Queue of terminate outcomes. Empty ⇒ always succeed. Used to
    /// exercise the supervisor's failure-propagation paths.
    pub terminate_script: std::collections::VecDeque<std::result::Result<(), String>>,
    /// Queue of post-ready outcomes per server.
    pub post_ready_script:
        std::collections::HashMap<ServerName, std::collections::VecDeque<PostReadyStatus>>,
    /// Queue of probe_meta outcomes — if empty we return the default meta.
    pub probe_script: std::collections::HashMap<
        ServerName,
        std::collections::VecDeque<std::result::Result<JackMeta, String>>,
    >,
}

#[derive(Clone, Default)]
pub struct MockBackend {
    pub inner: Arc<Mutex<MockBackendInner>>,
}

impl MockBackend {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a spawn outcome. `Ok(())` makes the server Running; `Err(msg)`
    /// makes the spawn fail with that message.
    pub fn queue_spawn_result(&self, result: std::result::Result<(), String>) {
        self.inner.lock().unwrap().spawn_script.push_back(result);
    }

    /// Queue a terminate outcome. `Err(msg)` simulates a real-world
    /// termination failure (e.g. jackd we can't discover the PID of).
    pub fn queue_terminate_result(&self, result: std::result::Result<(), String>) {
        self.inner
            .lock()
            .unwrap()
            .terminate_script
            .push_back(result);
    }

    /// Queue a post-ready verdict for a server. The next `post_ready_status`
    /// call for that server pops this.
    pub fn queue_post_ready(&self, name: &ServerName, status: PostReadyStatus) {
        self.inner
            .lock()
            .unwrap()
            .post_ready_script
            .entry(name.clone())
            .or_default()
            .push_back(status);
    }

    /// Queue a `probe_meta` outcome for a server.
    pub fn queue_probe_result(
        &self,
        name: &ServerName,
        result: std::result::Result<JackMeta, String>,
    ) {
        self.inner
            .lock()
            .unwrap()
            .probe_script
            .entry(name.clone())
            .or_default()
            .push_back(result);
    }

    /// Directly set the meta returned by `probe_meta` when no script entries
    /// are queued. Useful when the test just wants a stable default.
    pub fn set_default_meta(&self, name: &ServerName, meta: JackMeta) {
        self.inner
            .lock()
            .unwrap()
            .meta_for
            .insert(name.clone(), meta);
    }

    pub fn calls(&self) -> Vec<MockCall> {
        self.inner.lock().unwrap().calls.clone()
    }

    pub fn call_count(&self) -> usize {
        self.inner.lock().unwrap().calls.len()
    }
}

impl JackBackend for MockBackend {
    fn spawn(&mut self, name: &ServerName, config: &JackConfig) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        inner
            .calls
            .push(MockCall::Spawn(name.clone(), config.clone()));
        let outcome = inner.spawn_script.pop_front().unwrap_or(Ok(()));
        match outcome {
            Ok(()) => {
                inner.running.insert(name.clone());
                // Seed a default meta if one hasn't been set explicitly.
                inner
                    .meta_for
                    .entry(name.clone())
                    .or_insert_with(|| JackMeta {
                        sample_rate: config.sample_rate,
                        buffer_size: config.buffer_size,
                        capture_port_count: config.capture_channels as usize,
                        playback_port_count: config.playback_channels as usize,
                        hw_name: format!("mock:{}", name),
                    });
                Ok(())
            }
            Err(msg) => Err(anyhow::anyhow!(msg)),
        }
    }

    fn terminate(&mut self, name: &ServerName) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        inner.calls.push(MockCall::Terminate(name.clone()));
        // Consume a scripted outcome first so tests can exercise the
        // error-propagation paths. On Err we keep `running` populated so
        // the supervisor's follow-up `is_socket_present` still sees the
        // zombie — matching the real-world case where we couldn't SIGKILL.
        if let Some(outcome) = inner.terminate_script.pop_front() {
            match outcome {
                Ok(()) => {
                    inner.running.remove(name);
                    Ok(())
                }
                Err(msg) => Err(anyhow::anyhow!(msg)),
            }
        } else {
            inner.running.remove(name);
            Ok(())
        }
    }

    fn probe_meta(&mut self, name: &ServerName) -> Result<JackMeta> {
        let mut inner = self.inner.lock().unwrap();
        inner.calls.push(MockCall::ProbeMeta(name.clone()));
        if let Some(queue) = inner.probe_script.get_mut(name) {
            if let Some(item) = queue.pop_front() {
                return item.map_err(|e| anyhow::anyhow!(e));
            }
        }
        if !inner.running.contains(name) {
            return Err(anyhow::anyhow!("server '{}' not running", name));
        }
        inner
            .meta_for
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no meta configured for '{}'", name))
    }

    fn is_socket_present(&self, name: &ServerName) -> bool {
        self.inner.lock().unwrap().running.contains(name)
    }

    fn post_ready_status(&mut self, name: &ServerName) -> PostReadyStatus {
        let mut inner = self.inner.lock().unwrap();
        inner.calls.push(MockCall::PostReadyStatus(name.clone()));
        if let Some(queue) = inner.post_ready_script.get_mut(name) {
            if let Some(item) = queue.pop_front() {
                // Simulate jackd dying post-ready by clearing the running set.
                if !matches!(item, PostReadyStatus::Healthy) {
                    inner.running.remove(name);
                }
                return item;
            }
        }
        PostReadyStatus::Healthy
    }

    fn forget(&mut self, name: &ServerName) {
        let mut inner = self.inner.lock().unwrap();
        inner.calls.push(MockCall::Forget(name.clone()));
        inner.running.remove(name);
        inner.meta_for.remove(name);
        inner.probe_script.remove(name);
        inner.post_ready_script.remove(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name() -> ServerName {
        ServerName::from("test")
    }

    #[test]
    fn mock_backend_default_spawn_makes_server_probeable() {
        let mut backend = MockBackend::new();
        backend.spawn(&name(), &JackConfig::test_default()).unwrap();
        assert!(backend.is_socket_present(&name()));
        let meta = backend.probe_meta(&name()).unwrap();
        assert_eq!(meta.sample_rate, 48_000);
        assert_eq!(meta.buffer_size, 128);
    }

    #[test]
    fn mock_backend_spawn_error_leaves_server_not_running() {
        let mut backend = MockBackend::new();
        backend.queue_spawn_result(Err("simulated".into()));
        let err = backend
            .spawn(&name(), &JackConfig::test_default())
            .unwrap_err();
        assert!(err.to_string().contains("simulated"));
        assert!(!backend.is_socket_present(&name()));
    }

    #[test]
    fn mock_backend_post_ready_failure_clears_running() {
        let mut backend = MockBackend::new();
        backend.queue_post_ready(
            &name(),
            PostReadyStatus::DriverFailure("Broken pipe".into()),
        );
        backend.spawn(&name(), &JackConfig::test_default()).unwrap();
        assert!(backend.is_socket_present(&name()));
        let status = backend.post_ready_status(&name());
        assert!(matches!(status, PostReadyStatus::DriverFailure(_)));
        assert!(!backend.is_socket_present(&name()));
    }

    #[test]
    fn mock_backend_records_call_order() {
        let mut backend = MockBackend::new();
        backend.spawn(&name(), &JackConfig::test_default()).unwrap();
        backend.post_ready_status(&name());
        let _ = backend.probe_meta(&name());
        backend.terminate(&name()).unwrap();
        backend.forget(&name());
        let calls = backend.calls();
        assert!(matches!(calls[0], MockCall::Spawn(_, _)));
        assert!(matches!(calls[1], MockCall::PostReadyStatus(_)));
        assert!(matches!(calls[2], MockCall::ProbeMeta(_)));
        assert!(matches!(calls[3], MockCall::Terminate(_)));
        assert!(matches!(calls[4], MockCall::Forget(_)));
    }

    #[test]
    fn mock_backend_forget_clears_running_and_scripts() {
        let mut backend = MockBackend::new();
        backend.spawn(&name(), &JackConfig::test_default()).unwrap();
        backend.queue_post_ready(&name(), PostReadyStatus::SocketVanished);
        backend.forget(&name());
        assert!(!backend.is_socket_present(&name()));
        assert!(backend
            .inner
            .lock()
            .unwrap()
            .post_ready_script
            .get(&name())
            .is_none());
    }

    #[test]
    fn mock_backend_probe_script_overrides_default_meta() {
        let mut backend = MockBackend::new();
        backend.spawn(&name(), &JackConfig::test_default()).unwrap();
        backend.queue_probe_result(&name(), Err("simulated probe failure".into()));
        let err = backend.probe_meta(&name()).unwrap_err();
        assert!(err.to_string().contains("simulated"));
    }
}
