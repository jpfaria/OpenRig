//! Responsibility: points a chain at one endpoint of a binding.

use serde::{Deserialize, Serialize};

/// #717: a reference to one of a chain's already-bound output endpoints,
/// identifying the endpoint by its binding id + endpoint name (a name alone is
/// not unique across the chain's bindings). Used to route the dedicated DI
/// stream to a chosen output. Travels with the chain in `project.openrig`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiOutputRef {
    pub binding_id: String,
    pub endpoint: String,
}

/// #323: a reference to one of a chain's already-bound I/O endpoints
/// (binding id + endpoint name — a name alone is not unique across bindings).
/// A looper uses one to say WHERE it records its dry signal from (an input
/// endpoint) and WHERE its playback goes (an output endpoint). Same shape as
/// `DiOutputRef`, kept separate so the looper does not couple to the DI type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EndpointRef {
    pub binding_id: String,
    pub endpoint: String,
}
