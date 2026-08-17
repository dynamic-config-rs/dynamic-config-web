//! A request-scoped configuration snapshot for Actix Web.
//!
//! ```no_run
//! use actix_web::{get, App, HttpServer};
//! use dynamic_config_actix::{Config, DynamicConfig};
//! use dynamic_config_web_core::sections;
//! # use dynamic_config::dynamic_config;
//! # use serde::Deserialize;
//! # #[dynamic_config] #[derive(Deserialize)] struct Server { port: u16 }
//! # #[dynamic_config] #[derive(Deserialize)] struct Features { cache: bool }
//!
//! #[get("/")]
//! async fn index(server: Config<Server>, features: Config<Features>) -> String {
//!     // Both came out of one snapshot, taken when the request began.
//!     // `Sections::take` retries if a reload lands mid-read, so these
//!     // two cannot be different generations.
//!     format!("{} {}", server.port, features.cache)
//! }
//!
//! # fn build() {
//! HttpServer::new(|| {
//!     App::new()
//!         .wrap(DynamicConfig::new(sections![Server, Features]))
//!         .service(index)
//! });
//! # }
//! ```
//!
//! # What this is for
//!
//! Actix runs handlers across several worker threads, and `Server::current()`
//! is an atomic load that every worker can make without a lock. That part is
//! already right. What it does not give you is *two* sections that agree: a
//! reload landing between two reads lets one response mix generations.
//!
//! [`DynamicConfig`] reads every listed section once, before the handler
//! runs, and puts the result in the request's extensions. [`Config<T>`]
//! reads it back out.
//!
//! Note what is **not** here: no `web::Data<ServerConfig>`. Handing a
//! snapshot to the app factory freezes it at start-up, and each worker then
//! serves the configuration that existed when it was built.
//!
//! # What this is not
//!
//! It does not load configuration, watch files, or own a [`WatchHandle`].
//! That stays in the startup code that calls `init()` and holds the handles
//! for the life of the process.
//!
//! [`WatchHandle`]: https://docs.rs/dynamic-config/latest/dynamic_config/watch/struct.WatchHandle.html

#![forbid(unsafe_code)]
#![warn(missing_docs, missing_debug_implementations, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::any::Any;
use std::future::{ready, Ready};
use std::sync::Arc;

use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::StatusCode;
use actix_web::{Error, FromRequest, HttpMessage, HttpRequest, ResponseError};
use dynamic_config_web_core::{NotInScope, Sections, Snapshot};

pub use dynamic_config_web_core::{sections, NotInScope as OutOfScope, Sections as ConfigSections};

/// One section of this request's configuration.
///
/// ```no_run
/// # use dynamic_config_actix::Config;
/// # struct Database { host: String }
/// async fn handler(db: Config<Database>) -> String {
///     db.host.clone()
/// }
/// ```
///
/// Extracting the same type twice in one handler answers the same `Arc`.
/// Extracting one the middleware was not given answers `500` — see
/// [`SnapshotMissing`].
pub struct Config<T>(pub Arc<T>);

impl<T> Clone for Config<T> {
    /// Hand-written: cloning an `Arc` never needs `T: Clone`.
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> std::fmt::Debug for Config<T> {
    /// The type's name, never the section's contents — a section holds
    /// credentials, and `?config` is how one reaches a log line.
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

impl<T> FromRequest for Config<T>
where
    T: Any + Send + Sync,
{
    type Error = Error;
    type Future = Ready<Result<Self, Error>>;

    fn from_request(request: &HttpRequest, _payload: &mut actix_web::dev::Payload) -> Self::Future {
        ready(from_parts(request).map_err(Into::into))
    }
}

fn from_parts<T: Any + Send + Sync>(request: &HttpRequest) -> Result<Config<T>, SnapshotMissing> {
    let extensions = request.extensions();
    let snapshot = extensions
        .get::<Snapshot>()
        .ok_or(SnapshotMissing::NoMiddleware)?;

    snapshot
        .require::<T>()
        .map(Config)
        .map_err(SnapshotMissing::Section)
}

/// Why a [`Config`] extractor could not answer.
///
/// Every variant is a wiring mistake rather than anything a client did,
/// which is why they are all `500`.
#[derive(Debug, Clone, Copy)]
pub enum SnapshotMissing {
    /// No [`DynamicConfig`] middleware ran for this request.
    NoMiddleware,
    /// It ran, and this section was not in what it took.
    Section(NotInScope),
}

impl std::fmt::Display for SnapshotMissing {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoMiddleware => formatter.write_str(
                "no configuration snapshot on this request: add \
                 `.wrap(DynamicConfig::new(sections![..]))` to the app",
            ),
            Self::Section(why) => write!(formatter, "{why}"),
        }
    }
}

impl std::error::Error for SnapshotMissing {}

impl ResponseError for SnapshotMissing {
    fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }

    fn error_response(&self) -> actix_web::HttpResponse {
        // Overridden, because the default renders `Display` into the body
        // and `Display` names an internal type path. That detail is for
        // whoever reads the logs; the client is told only that the server
        // is misconfigured. The axum adapter answers the same way.
        actix_web::HttpResponse::build(self.status_code())
            .content_type("text/plain; charset=utf-8")
            .body("configuration is not wired for this handler")
    }
}

/// The snapshot this request began with, for code holding an
/// [`HttpRequest`] rather than using the extractor.
///
/// Answers a clone, because actix hands out extensions behind a `RefCell`
/// and a borrow could not outlive the call.
///
/// # Errors
///
/// [`SnapshotMissing::NoMiddleware`] when the middleware did not run.
pub fn snapshot(request: &HttpRequest) -> Result<Snapshot, SnapshotMissing> {
    request
        .extensions()
        .get::<Snapshot>()
        .cloned()
        .ok_or(SnapshotMissing::NoMiddleware)
}

/// Takes one snapshot per request and puts it in the request's extensions.
///
/// ```no_run
/// # use actix_web::App;
/// # use dynamic_config_actix::DynamicConfig;
/// # use dynamic_config_web_core::Sections;
/// # fn build() {
/// # let sections = || Sections::new();
/// App::new().wrap(DynamicConfig::new(sections()));
/// # }
/// ```
///
/// `HttpServer::new` takes a factory and calls it once per worker thread,
/// so either build the [`Sections`] inside the closure — free, since it
/// holds closures over process-wide configuration — or build one list and
/// clone it in:
///
/// ```no_run
/// # use actix_web::{App, HttpServer};
/// # use dynamic_config_actix::DynamicConfig;
/// # use dynamic_config_web_core::Sections;
/// # fn build() {
/// let configuration = DynamicConfig::new(Sections::new());
///
/// HttpServer::new(move || App::new().wrap(configuration.clone()));
/// # }
/// ```
pub struct DynamicConfig {
    sections: Arc<Sections>,
}

impl DynamicConfig {
    /// Builds the middleware over the sections a request should read.
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

impl std::fmt::Debug for DynamicConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DynamicConfig")
            .field("sections", &self.sections.names())
            .finish()
    }
}

impl Clone for DynamicConfig {
    fn clone(&self) -> Self {
        Self {
            sections: Arc::clone(&self.sections),
        }
    }
}

impl<S, B> Transform<S, ServiceRequest> for DynamicConfig
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = SnapshotMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(SnapshotMiddleware {
            service,
            sections: Arc::clone(&self.sections),
        }))
    }
}

/// The service [`DynamicConfig`] wraps an application in.
pub struct SnapshotMiddleware<S> {
    service: S,
    sections: Arc<Sections>,
}

impl<S> std::fmt::Debug for SnapshotMiddleware<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SnapshotMiddleware")
            .field("sections", &self.sections.names())
            .finish_non_exhaustive()
    }
}

impl<S, B> Service<ServiceRequest> for SnapshotMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    // The inner future, unboxed: nothing here runs after `call`, so there
    // is nothing to wrap and no allocation to make per request.
    type Future = S::Future;

    forward_ready!(service);

    fn call(&self, request: ServiceRequest) -> Self::Future {
        // Taken *before* the extensions are borrowed: `take()` calls
        // user-supplied readers, and actix hands out extensions behind a
        // `RefCell` that panics on an overlapping borrow.
        let taken = self.sections.take();

        // Merged rather than inserted, because middleware nests: an
        // `App::wrap` and a `scope().wrap()` may each carry a list, and a
        // bare insert would erase the outer one.
        let merged = match request.extensions_mut().remove::<Snapshot>() {
            Some(outer) => outer.merged_with(taken),
            None => taken,
        };

        request.extensions_mut().insert(merged);

        self.service.call(request)
    }
}
