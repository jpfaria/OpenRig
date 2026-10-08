//! Responsibility: persists the bindings made in the setup wizard without a project.
//!
//! With a project open the binding commands persist through the dispatcher.
//! On first run there is no project, so the bindings live only in the GUI's
//! `AppConfig` snapshot until the user leaves the I/O step, when they are
//! written here, off the GUI thread.

use infra_filesystem::{FilesystemStorage, IoBinding};

pub(crate) fn persist(bindings: Vec<IoBinding>) {
    application::persist_worker::run(move || {
        let mut config = FilesystemStorage::load_app_config().unwrap_or_default();
        config.io_bindings = bindings;
        if let Err(e) = FilesystemStorage::save_app_config(&config) {
            log::error!("failed to persist the wizard's I/O bindings: {e}");
        }
    });
}
