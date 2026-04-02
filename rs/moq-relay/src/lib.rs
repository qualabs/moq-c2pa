//! Embeddable MoQ relay for connecting publishers to subscribers.
//!
//! The relay is content-agnostic: it forwards live data without
//! interpreting it, so it works equally well for media, sensor telemetry,
//! or any other stream. Clustering, JWT authentication, WebSocket
//! fallback, and an HTTP API are all included.
//!
//! See `main.rs` for a complete example of how these pieces fit together.

mod auth;
#[cfg(feature = "c2pa")]
mod c2pa;
mod cluster;
mod config;
mod connection;
mod upstream;
mod web;
#[cfg(feature = "websocket")]
mod websocket;

/// The relay needs higher stream limits than the library default
/// to handle many concurrent subscriptions across connections.
pub const DEFAULT_MAX_STREAMS: u64 = 10_000;

pub use auth::*;
#[cfg(feature = "c2pa")]
pub use c2pa::*;
pub use cluster::*;
pub use config::*;
pub use connection::*;
pub use upstream::*;
pub use web::*;
