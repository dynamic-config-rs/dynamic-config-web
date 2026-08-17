//! Two sections that must agree, over Actix Web.
//!
//! ```sh
//! cargo run -p dynamic-config-actix --example two_sections
//! ```
//!
//! The engine's own `actix_hello` example reads one section per handler with
//! `current()`, and makes the point that configuration does not belong in
//! `web::Data`. This is the case that needs a crate: a handler reading *two*
//! sections, where a reload between the reads would serve one response from
//! two generations.
//!
//! Loading and watching stay in `main`. This crate owns no lifecycle.

use std::sync::Arc;
use std::time::Duration;

use actix_web::{test, web, App};
use dynamic_config::dynamic_config;
use dynamic_config_actix::{Config, DynamicConfig};
use dynamic_config_web_core::sections;
use serde::Deserialize;

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
async fn index(server: Config<Server>, features: Config<Features>) -> String {
    format!(
        r#"{{"host":"{}","port":{},"cache":{},"generation":{}}}"#,
        server.host, server.port, features.cache, features.generation
    )
}

/// Reads the same section twice, and reports whether it got one value.
async fn twice(first: Config<Features>, second: Config<Features>) -> String {
    format!(
        r#"{{"first":{},"second":{},"same_object":{}}}"#,
        first.generation,
        second.generation,
        Arc::ptr_eq(&first.0, &second.0)
    )
}

#[actix_web::main]
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

    // `HttpServer::new` would take this closure once per worker thread.
    // Building the sections inside it is free: they are closures over the
    // same process-wide configuration, not a copy of it.
    let app = test::init_service(
        App::new()
            .wrap(DynamicConfig::new(sections![Server, Features]))
            .route("/", web::get().to(index))
            .route("/twice", web::get().to(twice)),
    )
    .await;

    show("serving");
    println!("  GET /       → {}", call(&app, "/").await);
    println!("  GET /twice  → {}", call(&app, "/twice").await);

    show("a deployment edits the file");
    write(&path, 2)?;
    Server::builder("server").file(file).reload()?;
    Features::builder("features").file(file).reload()?;

    println!("  GET /       → {}", call(&app, "/").await);
    println!("\nBoth numbers moved together: one request reads once.");

    show("what is not here");
    println!("  No `web::Data<Server>`. Handing a snapshot to the app factory");
    println!("  freezes it at start-up, and every worker then serves the");
    println!("  configuration that existed when it was built.");

    Ok(())
}

/// One request through the app, without a socket.
async fn call<S, B>(app: &S, uri: &str) -> String
where
    S: actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse<B>,
        Error = actix_web::Error,
    >,
    B: actix_web::body::MessageBody,
{
    let response = test::call_service(app, test::TestRequest::get().uri(uri).to_request()).await;
    let body = test::read_body(response).await;

    String::from_utf8_lossy(&body).into_owned()
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
