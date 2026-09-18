//! Responsibility: routes the in-app update offer to the file that owns each job
//!
//! - [`version`] — is a release tag newer than the running build
//! - [`release`] — reading the tag out of GitHub's latest-release JSON
//! - [`offer`] — the release the running app is offered
//! - [`running_version`] — the version the check compares against
//! - [`fetch`] — downloading that JSON
//! - [`installer`] — running the macOS installer in Terminal
//! - [`wiring`] — the launcher's version label/button

mod fetch;
mod installer;
mod offer;
mod release;
mod running_version;
mod version;
mod wiring;

pub(crate) use wiring::wire_app_update;
