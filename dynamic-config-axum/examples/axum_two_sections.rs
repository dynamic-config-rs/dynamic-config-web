//! Two sections that must agree, over axum.
//!
//! ```sh
//! cargo run -p dynamic-config-axum --example axum_two_sections
//! ```
//!
//! The engine's own `axum_hello` example reads one section per handler with
//! `current()`, which is right and needs no crate. This is the case that
//! does need one: a handler reading *two* sections, where a reload landing
//! between the reads would otherwise serve one response from two
//! generations.
//!
//! Nothing about loading or watching changes. That stays in `main`, where
//! it already was — this crate owns no lifecycle.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::Request;
use axum::routing::get;
use axum::{Json, Router};
use dynamic_config::dynamic_config;
use dynamic_config_axum::{Config, SnapshotLayer};
use dynamic_config_web_core::sections;
use http_body_util::BodyExt;
use serde::Deserialize;
use serde_json::json;
use tower::ServiceExt;

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

/// Reads both sections.
///
/// `generation` is written into both halves of the document, so a torn read
/// would show two different numbers in one response.
async fn index(
    Config(server): Config<Server>,
    Config(features): Config<Features>,
) -> Json<serde_json::Value> {
    Json(json!({
        "host": server.host,
        "port": server.port,
        "cache": features.cache,
        "generation": features.generation,
    }))
}

/// Reads the same section twice, and reports whether it got one value.
async fn twice(first: Config<Features>, second: Config<Features>) -> Json<serde_json::Value> {
    Json(json!({
        "first": first.generation,
        "second": second.generation,
        "same_object": Arc::ptr_eq(&first.0, &second.0),
    }))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("config.json");
    let file = path.to_str().expect("a utf-8 path");

    write(&path, 1)?;

    // Load before serving, and fail here rather than on the first request.
    Server::builder("server").file(file).init()?;
    Features::builder("features").file(file).init()?;

    // Held for the length of `main`: dropping a handle stops that watcher.
    let _watchers = [
        Server::builder("server")
            .file(file)
            .watch(Duration::from_millis(50))?,
        Features::builder("features")
            .file(file)
            .watch(Duration::from_millis(50))?,
    ];

    let app = Router::new()
        .route("/", get(index))
        .route("/twice", get(twice))
        .layer(SnapshotLayer::new(sections![Server, Features]));

    show("serving");
    println!("  GET /       → {}", call(&app, "/").await?);
    println!("  GET /twice  → {}", call(&app, "/twice").await?);

    show("a deployment edits the file");
    write(&path, 2)?;
    Server::builder("server").file(file).reload()?;
    Features::builder("features").file(file).reload()?;

    println!("  GET /       → {}", call(&app, "/").await?);
    println!("\nBoth numbers moved together: one request reads once.");

    show("without the layer");
    println!("  Two `Features::current()` calls in one handler are two reads.");
    println!("  A reload between them shows one response two generations —");
    println!("  which is what `same_object` above is checking for.");

    Ok(())
}

/// One request through the router, without a socket.
async fn call(app: &Router, uri: &str) -> Result<String, Box<dyn std::error::Error>> {
    let response = app
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
