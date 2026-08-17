//! A request-scoped configuration snapshot for Loco.
//!
//! ```no_run
//! use dynamic_config_loco::{sections, Config, DynamicConfig};
//! use loco_rs::prelude::*;
//! # use dynamic_config::dynamic_config;
//! # use serde::Deserialize;
//! # #[dynamic_config] #[derive(Deserialize)] struct Server { port: u16 }
//! # #[dynamic_config] #[derive(Deserialize)] struct Features { cache: bool }
//! # struct App;
//!
//! # impl App {
//! async fn initializers(_ctx: &AppContext) -> Result<Vec<Box<dyn Initializer>>> {
//!     Ok(vec![Box::new(DynamicConfig::new(sections![Server, Features]))])
//! }
//! # }
//! ```
//!
//! ```no_run
//! # use dynamic_config_loco::Config;
//! # use loco_rs::prelude::*;
//! # struct Server { port: u16 }
//! # struct Features { cache: bool }
//! async fn index(
//!     Config(server): Config<Server>,
//!     Config(features): Config<Features>,
//! ) -> Result<Response> {
//!     // One reading, taken when the request began. `Sections::take`
//!     // retries if a reload lands mid-read, so these two cannot be
//!     // different generations.
//!     format::text(&format!("{} {}", server.port, features.cache))
//! }
//! ```
//!
//! # What this crate is
//!
//! Loco is axum underneath, so the layer and the extractor are
//! [`dynamic-config-axum`](https://docs.rs/dynamic-config-axum)'s, re-exported
//! here unchanged. What Loco adds is a place to register one — the
//! [`Initializer`] trait — and that registration is all this crate is.
//!
//! Writing it by hand is three lines in your own initializer, and doing so
//! is fine. This crate exists so that it is one line in `initializers`, and
//! so that there is somewhere to write down the two things below.
//!
//! # Loco's own configuration is a different thing
//!
//! Loco reads `config/development.yaml` into `ctx.config` at boot, and that
//! is where the database URL, the worker mode and the server port live.
//! None of it reloads, and none of it should: Loco binds its listener and
//! builds its connection pool from those values once.
//!
//! This crate is for the *other* half — the settings an operator changes
//! while the service runs. Keep them in their own file with their own
//! `#[dynamic_config]` sections, and leave `ctx.config` to Loco.
//!
//! # What this crate does not do
//!
//! It does not load configuration, watch files, or own a [`WatchHandle`].
//! Loco's `Hooks::boot` is where a service does that, before the router
//! exists.
//!
//! [`WatchHandle`]: https://docs.rs/dynamic-config/latest/dynamic_config/watch/struct.WatchHandle.html

#![forbid(unsafe_code)]
#![warn(missing_docs, missing_debug_implementations, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use async_trait::async_trait;

use dynamic_config_web_core::Sections;
use loco_rs::app::{AppContext, Initializer};
use loco_rs::Result;

// `axum::Router` itself: Loco's `AxumRouter` is a private alias for it
// inside `loco_rs::app`, so the trait's signature is spelled here the way
// the compiler sees it.
use axum::Router as AxumRouter;

pub use dynamic_config_axum::{Config, SnapshotLayer, SnapshotMissing};
pub use dynamic_config_web_core::{sections, NotInScope, Sections as ConfigSections, Snapshot};

/// The initializer that puts one reading on every request.
///
/// ```no_run
/// # use dynamic_config_loco::{DynamicConfig, ConfigSections};
/// # use loco_rs::app::Initializer;
/// let initializers: Vec<Box<dyn Initializer>> =
///     vec![Box::new(DynamicConfig::new(ConfigSections::new()))];
/// ```
///
/// Loco calls [`after_routes`](Initializer::after_routes) once, with the
/// router it has built, and this adds the layer to it — so every route the
/// application declares is covered, including the ones Loco adds itself.
pub struct DynamicConfig {
    layer: SnapshotLayer,
}

impl DynamicConfig {
    /// Builds the initializer over the sections a request should read.
    #[must_use]
    pub fn new(sections: Sections) -> Self {
        Self {
            layer: SnapshotLayer::new(sections),
        }
    }

    /// The same, boxed, for the `Vec<Box<dyn Initializer>>` Loco wants.
    ///
    /// ```no_run
    /// # use dynamic_config_loco::{DynamicConfig, ConfigSections};
    /// # use loco_rs::app::Initializer;
    /// let initializers: Vec<Box<dyn Initializer>> =
    ///     vec![DynamicConfig::boxed(ConfigSections::new())];
    /// ```
    #[must_use]
    pub fn boxed(sections: Sections) -> Box<dyn Initializer> {
        Box::new(Self::new(sections))
    }

    /// The layer this initializer installs.
    ///
    /// For an application that already has an initializer of its own and
    /// would rather add one layer than one more `Box<dyn Initializer>`:
    ///
    /// ```no_run
    /// # use dynamic_config_loco::{DynamicConfig, ConfigSections};
    /// # use axum::Router;
    /// # let router: Router = Router::new();
    /// let configuration = DynamicConfig::new(ConfigSections::new());
    ///
    /// let router = router.layer(configuration.layer());
    /// ```
    #[must_use]
    pub fn layer(&self) -> SnapshotLayer {
        self.layer.clone()
    }

    /// The type names it will take, in order.
    #[must_use]
    pub fn names(&self) -> Vec<&'static str> {
        self.layer.names()
    }
}

impl std::fmt::Debug for DynamicConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DynamicConfig")
            .field("sections", &self.layer.names())
            .finish()
    }
}

#[async_trait]
impl Initializer for DynamicConfig {
    /// What Loco prints when it lists what is installed.
    fn name(&self) -> String {
        "dynamic-config".to_string()
    }

    /// Adds the layer to the router Loco has built.
    async fn after_routes(&self, router: AxumRouter, _context: &AppContext) -> Result<AxumRouter> {
        // Cloned rather than moved: `after_routes` takes `&self`, and the
        // layer is an `Arc` around the section list — so this is a refcount
        // bump, not a second copy of anything.
        Ok(router.layer(self.layer.clone()))
    }
}
