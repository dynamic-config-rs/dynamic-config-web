//! A request-scoped configuration snapshot for axum.
//!
//! ```no_run
//! use axum::{routing::get, Router};
//! use dynamic_config_axum::{Config, SnapshotLayer};
//! use dynamic_config_web_core::sections;
//! # use dynamic_config::dynamic_config;
//! # use serde::Deserialize;
//! # #[dynamic_config] #[derive(Deserialize)] struct Server { port: u16 }
//! # #[dynamic_config] #[derive(Deserialize)] struct Features { cache: bool }
//!
//! async fn index(
//!     Config(server): Config<Server>,
//!     Config(features): Config<Features>,
//! ) -> String {
//!     // Both came out of one snapshot, taken when the request began.
//!     // `Sections::take` retries if a reload lands mid-read, so these
//!     // two cannot be different generations.
//!     format!("{} {}", server.port, features.cache)
//! }
//!
//! let app: Router = Router::new()
//!     .route("/", get(index))
//!     .layer(SnapshotLayer::new(sections![Server, Features]));
//! ```
//!
//! # What this is for
//!
//! `Server::current()` is an atomic load, and calling it in a handler is
//! correct. Calling it *twice*, or calling it for two sections, is where a
//! reload landing mid-request lets one response mix generations.
//!
//! [`SnapshotLayer`] reads every listed section once, before the handler
//! runs, and stores the result in the request's extensions. [`Config<T>`]
//! reads it back out. Request extensions are axum's request scope, so
//! nothing here is thread-local and nothing has to be undone afterwards.
//!
//! # What this is not
//!
//! It does not load configuration, watch files, or own a [`WatchHandle`].
//! That stays where it already is — in the startup code that calls
//! `init()` and holds the handles for the life of the process.
//!
//! [`WatchHandle`]: https://docs.rs/dynamic-config/latest/dynamic_config/watch/struct.WatchHandle.html

//!
//! # Long-lived connections
//!
//! A WebSocket upgrade begins as an HTTP request, so `Config<T>`
//! extracted at upgrade time is correct *for the handshake* — and wrong
//! as the connection's configuration for life. Do not move the `Arc`
//! into the `on_upgrade` future as "the config"; inside the socket loop
//! read `T::current()` per iteration or per message batch, and treat a
//! change as an event if the protocol wants one. The same applies to SSE
//! and long streaming bodies.

#![forbid(unsafe_code)]
#![warn(missing_docs, missing_debug_implementations, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::any::Any;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use dynamic_config_web_core::{NotInScope, Snapshot};

pub use dynamic_config_web_core::{sections, NotInScope as OutOfScope, Sections as ConfigSections};

/// One section of this request's configuration.
///
/// ```no_run
/// # use dynamic_config_axum::Config;
/// # struct Database { host: String }
/// async fn handler(Config(db): Config<Database>) -> String {
///     db.host.clone()
/// }
/// ```
///
/// Extracting the same type twice in one handler answers the same `Arc`.
/// Extracting one the layer was not given is a wiring mistake and answers
/// `500` — see [`SnapshotMissing`].
pub struct Config<T>(pub Arc<T>);

impl<T> Clone for Config<T> {
    /// Hand-written: cloning an `Arc` never needs `T: Clone`, and a
    /// derive would demand it of every section.
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> std::fmt::Debug for Config<T> {
    /// The type's name, never the section's contents.
    ///
    /// A configuration section holds credentials, and `?config` in a
    /// `tracing` call is exactly how one reaches a log line. `Snapshot`
    /// holds the same line; this is the extractor keeping it.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("Config")
            .field(&std::any::type_name::<T>())
            .finish()
    }
}

impl<T> std::ops::Deref for Config<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S, T> FromRequestParts<S> for Config<T>
where
    S: Send + Sync,
    T: Any + Send + Sync,
{
    type Rejection = SnapshotMissing;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let snapshot = parts
            .extensions
            .get::<Snapshot>()
            .ok_or(SnapshotMissing::NoLayer)?;

        snapshot
            .require::<T>()
            .map(Config)
            .map_err(SnapshotMissing::Section)
    }
}

/// Why a [`Config`] extractor could not answer.
///
/// Every variant is a wiring mistake rather than anything a client did,
/// which is why they are all `500`: a request that asks for a section the
/// application never registered would be wrong however it was sent.
#[derive(Debug, Clone, Copy)]
pub enum SnapshotMissing {
    /// No [`SnapshotLayer`] ran for this request.
    NoLayer,
    /// The layer ran, and this section was not in what it took.
    Section(NotInScope),
}

impl std::fmt::Display for SnapshotMissing {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoLayer => formatter.write_str(
                "no configuration snapshot on this request: add \
                 `.layer(SnapshotLayer::new(sections![..]))` to the router",
            ),
            Self::Section(why) => write!(formatter, "{why}"),
        }
    }
}

impl std::error::Error for SnapshotMissing {}

impl IntoResponse for SnapshotMissing {
    fn into_response(self) -> Response {
        // The detail names an internal type path, which is for whoever
        // reads the logs rather than for whoever sent the request. The
        // body says only that the server is misconfigured; `Display`
        // carries the rest.
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "configuration is not wired for this handler",
        )
            .into_response()
    }
}

pub use dynamic_config_tower::{SnapshotLayer, SnapshotService};

/// The snapshot this request began with, for code that has the parts in
/// hand rather than an extractor — another middleware, or a handler that
/// takes `Request` whole.
///
/// # Errors
///
/// [`SnapshotMissing::NoLayer`] when no [`SnapshotLayer`] ran.
pub fn snapshot(parts: &Parts) -> Result<&Snapshot, SnapshotMissing> {
    parts
        .extensions
        .get::<Snapshot>()
        .ok_or(SnapshotMissing::NoLayer)
}
