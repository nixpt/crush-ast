//! `MeshRequest` — the one type `crush-net` actually needs from `mesh-proto`
//! (exosphere/openko's point-to-point mesh wire format), defined locally
//! instead of taken as a dependency.
//!
//! `mesh-proto` lives in the private `exosphere` repo, which made it
//! impossible for anyone outside that org to build this (public) crate's
//! workspace at all, CI included — a `cargo check --workspace` needs every
//! member's manifest to load, private-repo path-dep or not. `crush-net` only
//! ever used this one struct, purely as a serde-serializable shape for its
//! own TLV framing (see `codec.rs`) — no other mesh-proto functionality.
//! Field-for-field identical to `mesh_proto::MeshRequest` so JSON wire bytes
//! stay compatible with whatever on the other end still uses the real type.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshRequest {
    pub id: String,
    pub method: String,
    pub params: serde_json::Value,
    pub caller_did: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
}
