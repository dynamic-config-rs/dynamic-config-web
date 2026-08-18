//! One reading of configuration per request, as a plain `tower` layer.
//!
//! This is the layer [`dynamic-config-axum`](https://docs.rs/dynamic-config-axum)
//! is built on, published on its own because nothing in it is axum's:
//! it wraps any `tower::Service` over an `http::Request`, takes one
//! [`Snapshot`] when the request begins, and puts it in the request's
//! extensions. tonic, plain hyper, or any tower stack can use it
//! directly; axum adds only its extractor on top.
//!
//! ```no_run
//! use dynamic_config_tower::SnapshotLayer;
//! use dynamic_config_web_core::sections;
//! # use dynamic_config::dynamic_config;
//! # use serde::Deserialize;
//! # #[dynamic_config] #[derive(Deserialize)] struct Server { port: u16 }
//!
//! # fn wire<S>(service: S) -> impl tower::Layer<S> {
//! SnapshotLayer::new(sections![Server])
//! # }
//! ```
//!
//! Reading it back out is `request.extensions().get::<Snapshot>()`, and
//! [`Snapshot::require`] is the form whose error says which mistake was
//! made. The crate owns no lifecycle: loading, watching and the
//! `WatchHandle` stay in `main`, exactly as the web-core README says.
//!
//! # Long-lived connections
//!
//! A WebSocket upgrade, an SSE route and a streaming body all *begin* as
//! an HTTP request, so the layer gives each one a snapshot — and that is
//! correct for the handshake: whether to accept, from which
//! configuration, is a request-scoped question. What the snapshot must
//! not become is the connection's configuration for life. Inside the
//! connection loop, read fresh state per iteration or per message batch
//! — `T::current()` is that read — exactly as the Python package's
//! Limitations chapter puts it for ASGI: a connection that lives an hour
//! pinned to the configuration it opened with is the opposite of what
//! any of this is for.

#![forbid(unsafe_code)]
#![warn(missing_docs, missing_debug_implementations, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::sync::Arc;
use std::task::{Context, Poll};

use http::Request;
use tower::{Layer, Service};

pub use dynamic_config_web_core::{sections, NotInScope, Sections, Snapshot};

/// Takes one snapshot per request and puts it in the request's extensions.
///
/// Attach it *after* the routes it should cover, exactly as with any
/// tower layer: a layer wraps only what was there when it was added.
#[derive(Clone)]
pub struct SnapshotLayer {
    sections: Arc<Sections>,
}

impl SnapshotLayer {
    /// Builds the layer over the sections a request should read.
    #[must_use]
    pub fn new(sections: Sections) -> Self {
        Self {
            sections: Arc::new(sections),
        }
    }

    /// The type names it will take, in order.
    #[must_use]
    pub fn names(&self) -> Vec<&'static str> {
        self.sections.names()
    }
}

impl std::fmt::Debug for SnapshotLayer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SnapshotLayer")
            .field("sections", &self.sections.names())
            .finish()
    }
}

impl<S> Layer<S> for SnapshotLayer {
    type Service = SnapshotService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SnapshotService {
            inner,
            sections: Arc::clone(&self.sections),
        }
    }
}

/// The service [`SnapshotLayer`] wraps a stack in.
#[derive(Clone)]
pub struct SnapshotService<S> {
    inner: S,
    sections: Arc<Sections>,
}

impl<S> std::fmt::Debug for SnapshotService<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SnapshotService")
            .field("sections", &self.sections.names())
            .finish_non_exhaustive()
    }
}

impl<S, B> Service<Request<B>> for SnapshotService<S>
where
    S: Service<Request<B>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(context)
    }

    fn call(&mut self, mut request: Request<B>) -> Self::Future {
        // Once, here, before anything downstream runs. Every read in the
        // handler comes out of this one value.
        let taken = self.sections.take();

        // Merged rather than inserted, because layers nest: an outer
        // stack and an inner one may each carry a layer, the outer runs
        // first, and a bare `insert` here would erase what it took.
        let merged = match request.extensions_mut().remove::<Snapshot>() {
            Some(outer) => outer.merged_with(taken),
            None => taken,
        };

        request.extensions_mut().insert(merged);

        self.inner.call(request)
    }
}
