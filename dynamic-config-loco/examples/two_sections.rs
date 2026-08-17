//! Two sections that must agree, over Loco.
//!
//! ```sh
//! cargo run -p dynamic-config-loco --example two_sections
//! ```
//!
//! Loco is axum underneath, so the tearing this prevents is the axum
//! example's tearing and the fix is the same layer. What is Loco's own is
//! *where the layer gets installed*: an [`Initializer`], returned from
//! `Hooks::initializers`, which Loco calls once with the router it has
//! finished building.
//!
//! The two functions under "what a Loco application writes" below are
//! copy-pasteable into `src/app.rs`. Everything after them is this example
//! standing in for the parts of Loco that need a real application — the
//! boot sequence, and a socket.
//!
//! Loading and watching are not here either, and not because the example
//! omits them: they belong in `Hooks::boot`, before a router exists. This
//! crate owns no lifecycle.

use std::sync::Arc;
use std::time::Duration;

use axum::response::Response;
use axum::routing::get;
use axum::Router;
use dynamic_config::dynamic_config;
use dynamic_config_loco::{sections, Config, DynamicConfig};
use loco_rs::app::{AppContext, Initializer};
use loco_rs::controller::format;
use loco_rs::Result;
use serde::{Deserialize, Serialize};

#[dynamic_config]
#[derive(Debug, Deserialize)]
struct Server {
    host: String,
    port: u16,
}

#[dynamic_config]
#[derive(Debug, Deserialize)]
struct Features {
    cache: bool,
    generation: u32,
}

// ── what a Loco application writes ───────────────────────────────────────

/// `Hooks::initializers`, whole.
///
/// Loco takes this list at boot and calls `after_routes` on each one. The
/// `AppContext` is unused here: the sections are named by type, and nothing
/// about them comes from Loco's own configuration.
async fn initializers(_context: &AppContext) -> Result<Vec<Box<dyn Initializer>>> {
    Ok(vec![DynamicConfig::boxed(sections![Server, Features])])
}

/// A controller reading both sections.
///
/// `generation` is written into both halves of the document, so a response
/// assembled from two different readings would show two different numbers.
async fn index(
    Config(server): Config<Server>,
    Config(features): Config<Features>,
) -> Result<Response> {
    format::json(Reading {
        host: server.host.clone(),
        port: server.port,
        cache: features.cache,
        generation: features.generation,
    })
}

/// The same section asked for twice, in one signature.
async fn twice(first: Config<Features>, second: Config<Features>) -> Result<Response> {
    format::json(Twice {
        first: first.generation,
        second: second.generation,
        // Not merely equal — the same allocation. One reading, handed to
        // both extractors.
        same_object: Arc::ptr_eq(&first.0, &second.0),
    })
}

#[derive(Serialize)]
struct Reading {
    host: String,
    port: u16,
    cache: bool,
    generation: u32,
}

#[derive(Serialize)]
struct Twice {
    first: u32,
    second: u32,
    same_object: bool,
}

// ── what Loco does with it ───────────────────────────────────────────────

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("config.json");
    let file = path.to_str().expect("a utf-8 path");

    write(&path, 1)?;

    // `Hooks::boot`'s share: load before serving, so a broken document
    // fails startup rather than the first request.
    Server::builder("server").file(file).init()?;
    Features::builder("features").file(file).init()?;

    // Held for the length of `main` — dropping a handle stops that watcher.
    // In a Loco application these live wherever `boot` can keep them.
    let _watchers = [
        Server::builder("server")
            .file(file)
            .watch(Duration::from_millis(50))?,
        Features::builder("features")
            .file(file)
            .watch(Duration::from_millis(50))?,
    ];

    // An `AppContext` cannot be built from outside Loco — it is
    // `#[non_exhaustive]` — so this borrows the one Loco ships for tests.
    // A real application is handed its own.
    let context = loco_rs::tests_cfg::app::get_app_context().await;

    // `Hooks::routes`, roughly: Loco assembles this from its controllers.
    let mut router = Router::new()
        .route("/", get(index))
        .route("/twice", get(twice));

    show("boot");

    // Verbatim what `loco_rs::boot` runs, once the router exists.
    for initializer in initializers(&context).await? {
        println!("  initializer: {}", initializer.name());
        router = initializer.after_routes(router, &context).await?;
    }

    show("serving");
    println!("  GET /       → {}", call(&router, "/").await?);
    println!("  GET /twice  → {}", call(&router, "/twice").await?);

    show("a deployment edits the file");
    write(&path, 2)?;
    Server::builder("server").file(file).reload()?;
    Features::builder("features").file(file).reload()?;

    println!("  GET /       → {}", call(&router, "/").await?);
    println!("\nBoth numbers moved together: one request reads once.");

    show("Loco's own configuration is untouched");
    println!("  environment: {}", context.environment);
    println!("  `config/development.yaml` is read once at boot and stays read");
    println!("  once — the listener and the pool are built from it. This");
    println!("  layer is for the other half: what an operator turns while");
    println!("  the service runs.");

    Ok(())
}

/// One request through the router, without a socket.
async fn call(
    router: &Router,
    uri: &str,
) -> std::result::Result<String, Box<dyn std::error::Error>> {
    use axum::body::Body;
    use axum::extract::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let response = router
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty())?)
        .await?;

    let bytes = response.into_body().collect().await?.to_bytes();

    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn write(path: &std::path::Path, generation: u32) -> std::io::Result<()> {
    std::fs::write(
        path,
        format!(
            r#"{{"server": {{"host": "db-{generation}.internal", "port": {port}}},
                 "features": {{"cache": true, "generation": {generation}}}}}"#,
            port = 8080 + generation as u16,
        ),
    )
}

fn show(title: &str) {
    println!("\n{title}\n{}", "─".repeat(title.chars().count()));
}
