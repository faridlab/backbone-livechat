// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

// The module's DECLARED hand surface (user-owned; see
// `metaphor.codegen.yaml`): the public capability-token tree mounted
// BARE (tokens + fixed windows are the fence) and the officer tree
// the host mounts behind company_auth + the module write gate.
pub mod admin_routes;
pub mod public_routes;
pub use admin_routes::{livechat_admin_routes, LivechatActor, LivechatAdminState};
pub use public_routes::{livechat_public_routes, LivechatPublicState, PublicSessionView};
