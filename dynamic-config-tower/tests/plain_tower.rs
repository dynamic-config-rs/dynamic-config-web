//! The layer without axum anywhere in the tree.
//!
//! The crate's reason to exist: tonic, plain hyper, or any tower stack
//! gets one reading per request without adopting a web framework. This
//! drives the service through `tower::ServiceExt` over a hand-written
//! service — no router, no extractor, no framework.

use std::convert::Infallible;
use std::sync::Arc;

use dynamic_config::dynamic_config;
use dynamic_config_tower::{sections, Snapshot, SnapshotLayer};
use http::{Request, Response};
use serde::Deserialize;
use tower::{service_fn, Layer, Service, ServiceExt};

#[dynamic_config]
#[derive(Deserialize)]
struct Server {
    port: u16,
}

#[dynamic_config]
#[derive(Deserialize)]
struct Features {
    generation: u32,
}

fn install(port: u16, generation: u32) -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("config.json");

    std::fs::write(
        &path,
        format!(r#"{{"server": {{"port": {port}}}, "features": {{"generation": {generation}}}}}"#),
    )
    .expect("written");

    let file = path.to_str().expect("utf-8");

    Server::builder("server").file(file).init().expect("loads");
    Features::builder("features")
        .file(file)
        .init()
        .expect("loads");

    directory
}

#[tokio::test]
async fn a_plain_tower_service_reads_one_snapshot() {
    let _directory = install(8080, 1);

    let service = service_fn(|request: Request<()>| async move {
        // What any non-axum consumer writes: read the snapshot out of
        // the extensions, `require` for the error that names the fix.
        let snapshot = request
            .extensions()
            .get::<Snapshot>()
            .expect("the layer ran");

        let server = snapshot.require::<Server>().expect("listed and loaded");
        let features = snapshot.require::<Features>().expect("listed and loaded");

        Ok::<_, Infallible>(Response::new(format!(
            "{} {}",
            server.port, features.generation
        )))
    });

    let mut wired = SnapshotLayer::new(sections![Server, Features]).layer(service);

    let response = wired
        .ready()
        .await
        .expect("ready")
        .call(Request::new(()))
        .await
        .expect("answers");

    assert_eq!(response.into_body(), "8080 1");
}

#[tokio::test]
async fn two_requests_are_two_readings() {
    let _directory = install(8080, 1);

    let service = service_fn(|request: Request<()>| async move {
        let snapshot = request
            .extensions()
            .get::<Snapshot>()
            .expect("the layer ran");

        Ok::<_, Infallible>(Response::new(snapshot.get::<Server>().expect("loaded")))
    });

    let mut wired = SnapshotLayer::new(sections![Server]).layer(service);

    let first = wired
        .ready()
        .await
        .expect("ready")
        .call(Request::new(()))
        .await
        .expect("answers")
        .into_body();
    let second = wired
        .ready()
        .await
        .expect("ready")
        .call(Request::new(()))
        .await
        .expect("answers")
        .into_body();

    // Same installed document, so the same Arc — and a fresh take per
    // request, which is what "a snapshot is not a cache" means here.
    assert!(Arc::ptr_eq(&first, &second));
}
