//! Two sections that must agree, with no framework at all.
//!
//! ```sh
//! cargo run -p dynamic-config-tower --example tower_two_sections
//! ```
//!
//! The axum crate's `axum_two_sections` example is this same program with
//! a router; here the service is a bare `service_fn`, which is what a
//! tonic interceptor stack or a hand-rolled hyper server would wrap. The
//! layer's whole contract fits in two lines: one snapshot when the
//! request begins, read it back out of the extensions.

use std::convert::Infallible;
use std::time::Duration;

use dynamic_config::dynamic_config;
use dynamic_config_tower::{sections, Snapshot, SnapshotLayer};
use http::{Request, Response};
use serde::Deserialize;
use tower::{service_fn, Layer, Service, ServiceExt};

#[dynamic_config]
#[derive(Debug, Deserialize)]
struct Server {
    port: u16,
    generation: u32,
}

#[dynamic_config]
#[derive(Debug, Deserialize)]
struct Features {
    cache: bool,
    generation: u32,
}

/// `generation` is written into both halves of the document, so a torn
/// read would show two different numbers in one response.
fn write(path: &std::path::Path, generation: u32) -> std::io::Result<()> {
    std::fs::write(
        path,
        format!(
            r#"{{"server": {{"port": 8080, "generation": {generation}}},
                 "features": {{"cache": true, "generation": {generation}}}}}"#
        ),
    )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("config.json");
    let file = path.to_str().expect("a utf-8 path");

    write(&path, 1)?;

    // Load before serving; the watcher handle lives as long as `main`.
    Server::builder("server").file(file).init()?;
    Features::builder("features").file(file).init()?;

    let _watchers = [
        Server::builder("server")
            .file(file)
            .watch(Duration::from_millis(50))?,
        Features::builder("features")
            .file(file)
            .watch(Duration::from_millis(50))?,
    ];

    // The service any tower stack could be: the snapshot arrives in the
    // request's extensions, `require` names the fix when a section is
    // not listed.
    let service = service_fn(|request: Request<()>| async move {
        let snapshot = request
            .extensions()
            .get::<Snapshot>()
            .expect("the layer ran");

        let server = snapshot.require::<Server>().expect("listed and loaded");
        let features = snapshot.require::<Features>().expect("listed and loaded");

        Ok::<_, Infallible>(Response::new(format!(
            "port {} cache {} — server generation {} / features generation {}",
            server.port, features.cache, server.generation, features.generation
        )))
    });

    let mut wired = SnapshotLayer::new(sections![Server, Features]).layer(service);

    let answer = wired.ready().await?.call(Request::new(())).await?;

    println!("one request, one reading:\n  {}", answer.into_body());

    // A deployment edits the file; the next request sees both halves move
    // together, because the next snapshot is the first read of it.
    write(&path, 2)?;
    Server::builder("server").file(file).reload()?;
    Features::builder("features").file(file).reload()?;

    let answer = wired.ready().await?.call(Request::new(())).await?;

    println!("after the reload:\n  {}", answer.into_body());
    println!("\nBoth numbers always match: a reload between the two `require`s");
    println!("cannot tear a response, because neither read touches the engine.");

    Ok(())
}
