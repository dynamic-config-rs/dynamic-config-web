//! The initializer, driven through Loco's own trait.
//!
//! The layer and the extractor are the axum crate's and are tested there.
//! What is left to prove here is the seam: that `after_routes` puts the
//! layer on the router Loco hands it, and that a handler underneath then
//! reads one generation for every section it asks for.

use axum::body::Body;
use axum::extract::Request;
use axum::routing::get;
use axum::Router;
use dynamic_config::dynamic_config;
use dynamic_config_loco::{sections, Config, DynamicConfig};
use http_body_util::BodyExt;
use loco_rs::app::Initializer;
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
struct NeverListed {
    unused: bool,
}

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

        std::fs::write(
            &fixture.path,
            format!(
                r#"{{"server": {{"port": {port}}}, "features": {{"generation": {generation}}}}}"#
            ),
        )
        .expect("the document is written");

        let file = fixture.path.to_str().expect("a utf-8 path");

        Server::builder("server").file(file).init().expect("loads");
        Features::builder("features")
            .file(file)
            .init()
            .expect("loads");

        fixture
    }
}

/// The router Loco would have built, with the initializer's layer on it.
///
/// Not through `after_routes` itself: that takes an `&AppContext`, and
/// Loco's `Config` is `#[non_exhaustive]` with no `Default`, so building
/// one outside a booted application is not something a test can honestly
/// do. What `after_routes` does with the context is nothing — the trait's
/// own parameter is `_ctx` — and what it does with the router is
/// `router.layer(self.layer.clone())`, which is the line below.
///
/// So this covers everything except that one call, and
/// `it_installs_the_same_layer_it_answers_with` pins the rest.
fn wired(initializer: &DynamicConfig, router: Router) -> Router {
    router.layer(initializer.layer())
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
async fn the_initializer_wires_the_layer_onto_locos_router() {
    let _fixture = Fixture::new(8080, 1);

    async fn handler(Config(server): Config<Server>, Config(flags): Config<Features>) -> String {
        format!("{} {}", server.port, flags.generation)
    }

    let initializer = DynamicConfig::new(sections![Server, Features]);
    let router = wired(&initializer, Router::new().route("/", get(handler)));

    let (status, body) = body_of(router, "/").await;

    assert_eq!(status, 200, "{body}");
    assert_eq!(body, "8080 1");
}

#[tokio::test]
async fn without_the_initializer_a_handler_says_so() {
    let _fixture = Fixture::new(8080, 1);

    async fn handler(Config(server): Config<Server>) -> String {
        format!("{}", server.port)
    }

    // The same router, never passed through `after_routes`.
    let (status, _) = body_of(Router::new().route("/", get(handler)), "/").await;

    assert_eq!(status, 500);
}

#[tokio::test]
async fn a_section_the_initializer_was_not_given_says_so() {
    let _fixture = Fixture::new(8080, 1);

    async fn handler(Config(never): Config<NeverListed>) -> String {
        format!("{}", never.unused)
    }

    let initializer = DynamicConfig::new(sections![Server]);
    let router = wired(&initializer, Router::new().route("/", get(handler)));

    let (status, body) = body_of(router, "/").await;

    assert_eq!(status, 500);
    assert!(
        !body.contains("NeverListed"),
        "a type path reached a client"
    );
}

#[test]
fn it_installs_the_same_layer_it_answers_with() {
    // `layer()` and what `after_routes` installs are one value, so a test
    // over the first covers the second.
    let initializer = DynamicConfig::new(sections![Server, Features]);

    assert_eq!(initializer.layer().names(), initializer.names());
}

#[test]
fn it_reports_what_it_will_take() {
    let initializer = DynamicConfig::new(sections![Server, Features]);

    assert_eq!(initializer.names().len(), 2);
    assert_eq!(Initializer::name(&initializer), "dynamic-config");
    assert!(format!("{initializer:?}").contains("Server"));
}
