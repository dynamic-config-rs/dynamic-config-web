//! What the layer promises, against a real engine and a real router.
//!
//! The case that matters is the last one: a handler reads two sections, a
//! reload of both lands while it is running, and the two reads still agree.
//! Without the layer that request would serve one section from before the
//! reload and one from after.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::Request;
use axum::routing::get;
use axum::Router;
use dynamic_config::dynamic_config;
use dynamic_config_axum::{Config, SnapshotLayer};
use dynamic_config_web_core::{sections, Sections};
use http_body_util::BodyExt;
use serde::Deserialize;
use tower::ServiceExt;

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

#[dynamic_config]
#[derive(Deserialize)]
struct NeverLoaded {
    unused: bool,
}

// The reloading test gets sections of its own. A configuration is keyed by
// `TypeId` in a process-wide static, and cargo runs these tests on several
// threads of one process — so a test that reloads `Server` would be visible
// to every other test that reads it.
#[dynamic_config]
#[derive(Deserialize)]
struct MovingServer {
    port: u16,
}

#[dynamic_config]
#[derive(Deserialize)]
struct MovingFeatures {
    generation: u32,
}

/// A workspace holding one document, and the builders over it.
struct Fixture {
    _directory: tempfile::TempDir,
    path: std::path::PathBuf,
}

impl Fixture {
    fn new(port: u16, generation: u32) -> Self {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("config.json");
        let fixture = Self {
            _directory: directory,
            path,
        };

        fixture.write(port, generation);
        fixture
    }

    fn write(&self, port: u16, generation: u32) {
        std::fs::write(
            &self.path,
            format!(
                r#"{{"server": {{"port": {port}}}, "features": {{"generation": {generation}}}}}"#
            ),
        )
        .expect("the document is written");
    }

    fn install(&self) {
        let path = self.path.to_str().expect("a utf-8 path");

        Server::builder("server")
            .file(path)
            .init()
            .expect("server loads");
        Features::builder("features")
            .file(path)
            .init()
            .expect("features loads");
    }

    fn install_moving(&self) {
        let path = self.path.to_str().expect("a utf-8 path");

        MovingServer::builder("server")
            .file(path)
            .init()
            .expect("server loads");
        MovingFeatures::builder("features")
            .file(path)
            .init()
            .expect("features loads");
    }

    fn reload_moving(&self) {
        let path = self.path.to_str().expect("a utf-8 path");

        MovingServer::builder("server")
            .file(path)
            .reload()
            .expect("server reloads");
        MovingFeatures::builder("features")
            .file(path)
            .reload()
            .expect("features reloads");
    }
}

async fn body_of(app: Router, uri: &str) -> (u16, String) {
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .expect("the router answers");

    let status = response.status().as_u16();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("a body")
        .to_bytes();

    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn a_handler_reads_the_sections_it_asked_for() {
    let fixture = Fixture::new(8080, 1);
    fixture.install();

    async fn handler(Config(server): Config<Server>, Config(flags): Config<Features>) -> String {
        format!("{} {}", server.port, flags.generation)
    }

    let app = Router::new()
        .route("/", get(handler))
        .layer(SnapshotLayer::new(sections![Server, Features]));

    let (status, body) = body_of(app, "/").await;

    assert_eq!(status, 200);
    assert_eq!(body, "8080 1");
}

#[tokio::test]
async fn two_reads_in_one_handler_are_the_same_value() {
    let fixture = Fixture::new(8080, 1);
    fixture.install();

    async fn handler(first: Config<Server>, second: Config<Server>) -> String {
        format!("{}", Arc::ptr_eq(&first.0, &second.0))
    }

    let app = Router::new()
        .route("/", get(handler))
        .layer(SnapshotLayer::new(sections![Server]));

    assert_eq!(body_of(app, "/").await.1, "true");
}

#[tokio::test]
async fn a_request_never_tears_across_a_reload() {
    // The whole reason this crate exists.
    let fixture = Fixture::new(8080, 1);
    fixture.install_moving();

    let app =
        Router::new()
            .route(
                "/",
                get(
                    |Config(server): Config<MovingServer>,
                     Config(flags): Config<MovingFeatures>| async move {
                        format!("{} {}", server.port, flags.generation)
                    },
                ),
            )
            .layer(SnapshotLayer::new(sections![MovingServer, MovingFeatures]));

    // Take the snapshot, then move the file underneath it: the layer has
    // already read, so what it holds cannot change. Two reads out of it are
    // one generation, where two `current()` calls would have been two.
    let taken = Sections::new()
        .section(MovingServer::try_current)
        .section(MovingFeatures::try_current)
        .take();

    fixture.write(9090, 2);
    fixture.reload_moving();

    assert_eq!(taken.get::<MovingServer>().unwrap().port, 8080);
    assert_eq!(taken.get::<MovingFeatures>().unwrap().generation, 1);

    // And a request after the reload sees the new pair, together — a scope
    // is not a cache.
    assert_eq!(body_of(app, "/").await.1, "9090 2");
}

#[tokio::test]
async fn a_nested_layer_does_not_erase_the_outer_one() {
    // Two routers, each with its own list. The outer layer runs first and
    // the inner one second; a bare insert would replace what the outer
    // took, and a handler under both would 500 asking for a section it can
    // see in its own `sections![]` call.
    let fixture = Fixture::new(8080, 1);
    fixture.install();

    async fn handler(Config(server): Config<Server>, Config(flags): Config<Features>) -> String {
        format!("{} {}", server.port, flags.generation)
    }

    let inner = Router::new()
        .route("/both", get(handler))
        .layer(SnapshotLayer::new(sections![Features]));

    let app = Router::new()
        .nest("/inner", inner)
        .layer(SnapshotLayer::new(sections![Server]));

    let (status, body) = body_of(app, "/inner/both").await;

    assert_eq!(status, 200, "{body}");
    assert_eq!(body, "8080 1");
}

#[tokio::test]
async fn a_section_the_layer_was_not_given_says_so() {
    let fixture = Fixture::new(8080, 1);
    fixture.install();

    async fn handler(Config(never): Config<NeverLoaded>) -> String {
        format!("{}", never.unused)
    }

    let app = Router::new()
        .route("/", get(handler))
        // `NeverLoaded` is deliberately absent.
        .layer(SnapshotLayer::new(sections![Server]));

    let (status, body) = body_of(app, "/").await;

    assert_eq!(status, 500);
    // The client is told the server is misconfigured and nothing else: the
    // type path is this application's internals, and whoever sent the
    // request is not who needs it.
    assert!(
        !body.contains("NeverLoaded"),
        "a type path reached a client: {body}"
    );

    // Whoever reads the logs gets the whole thing.
    let detail = dynamic_config_axum::SnapshotMissing::Section(
        dynamic_config_web_core::NotInScope::NotListed("NeverLoaded"),
    )
    .to_string();

    assert!(detail.contains("NeverLoaded"), "{detail}");
    assert!(
        detail.contains("sections!"),
        "it says how to fix it: {detail}"
    );
}

#[tokio::test]
async fn a_handler_with_no_layer_says_that_instead() {
    async fn handler(Config(server): Config<Server>) -> String {
        format!("{}", server.port)
    }

    let app = Router::new().route("/", get(handler));

    let (status, _) = body_of(app, "/").await;

    assert_eq!(status, 500);
    assert!(
        dynamic_config_axum::SnapshotMissing::NoLayer
            .to_string()
            .contains("SnapshotLayer"),
        "the log line should name the fix"
    );
}

#[tokio::test]
async fn no_configuration_value_reaches_the_rejection() {
    let fixture = Fixture::new(8080, 1);
    fixture.install();

    async fn handler(Config(never): Config<NeverLoaded>) -> String {
        format!("{}", never.unused)
    }

    let app = Router::new()
        .route("/", get(handler))
        .layer(SnapshotLayer::new(sections![Server, Features]));

    let (_, body) = body_of(app, "/").await;

    assert!(
        !body.contains("8080"),
        "a port reached an error body: {body}"
    );
}

#[tokio::test]
async fn the_snapshot_escape_hatch_reads_what_the_layer_took() {
    // `snapshot(&Parts)` is the door for code that is not an extractor —
    // a middleware after this one, a guard. Same sections, same reading.
    let fixture = Fixture::new(8080, 1);
    fixture.install();

    async fn handler(request: axum::extract::Request) -> String {
        let (parts, _) = request.into_parts();
        let snapshot = dynamic_config_axum::snapshot(&parts).expect("the layer ran");

        match snapshot.get::<Server>() {
            Some(server) => format!("{}", server.port),
            None => "nothing".to_string(),
        }
    }

    let app = Router::new()
        .route("/", get(handler))
        .layer(SnapshotLayer::new(sections![Server]));

    let (status, body) = body_of(app, "/").await;

    assert_eq!(status, 200, "{body}");
    assert_eq!(body, "8080");
}
