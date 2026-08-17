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

/// The router Loco would have built, put through the initializer the way
/// Loco puts it: `after_routes`, with a real `AppContext`.
///
/// `AppContext` is `#[non_exhaustive]`, so nothing downstream can build
/// one. `tests_cfg` is Loco's own answer to that, and is why `testing` is a
/// dev-dependency of this crate — without it the test would have to
/// re-implement the line `after_routes` runs, and the seam under test would
/// become the test's own code.
async fn wired(initializer: &DynamicConfig, router: Router) -> Router {
    let context = loco_rs::tests_cfg::app::get_app_context().await;

    initializer
        .after_routes(router, &context)
        .await
        .expect("the initializer installs its layer")
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
    let router = wired(&initializer, Router::new().route("/", get(handler))).await;

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
    let router = wired(&initializer, Router::new().route("/", get(handler))).await;

    let (status, body) = body_of(router, "/").await;

    assert_eq!(status, 500);
    assert!(
        !body.contains("NeverListed"),
        "a type path reached a client"
    );
}

#[test]
fn it_installs_the_same_layer_it_answers_with() {
    // `layer()` is offered to applications that would rather add a layer
    // than one more `Box<dyn Initializer>`. What it hands back has to be
    // the layer `after_routes` installs, or the two routes into this crate
    // would not agree.
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
