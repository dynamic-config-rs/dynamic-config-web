//! What the middleware promises, against a real engine and a real app.
//!
//! The same six cases the axum crate answers, in Actix's vocabulary — the
//! two adapters make one promise, so they are held to one set of questions.

use std::sync::Arc;

use actix_web::{test, web, App};
use dynamic_config::dynamic_config;
use dynamic_config_actix::{Config, DynamicConfig};
use dynamic_config_web_core::{sections, Sections};
use serde::Deserialize;

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

// Its own sections, for the one test that reloads: a configuration lives in
// a process-wide static keyed by `TypeId`, and these tests share a process.
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

        Server::builder("server").file(path).init().expect("loads");
        Features::builder("features")
            .file(path)
            .init()
            .expect("loads");
    }

    fn install_moving(&self) {
        let path = self.path.to_str().expect("a utf-8 path");

        MovingServer::builder("server")
            .file(path)
            .init()
            .expect("loads");
        MovingFeatures::builder("features")
            .file(path)
            .init()
            .expect("loads");
    }

    fn reload_moving(&self) {
        let path = self.path.to_str().expect("a utf-8 path");

        MovingServer::builder("server")
            .file(path)
            .reload()
            .expect("reloads");
        MovingFeatures::builder("features")
            .file(path)
            .reload()
            .expect("reloads");
    }
}

#[actix_web::test]
async fn a_handler_reads_the_sections_it_asked_for() {
    let fixture = Fixture::new(8080, 1);
    fixture.install();

    async fn handler(server: Config<Server>, flags: Config<Features>) -> String {
        format!("{} {}", server.port, flags.generation)
    }

    let app = test::init_service(
        App::new()
            .wrap(DynamicConfig::new(sections![Server, Features]))
            .route("/", web::get().to(handler)),
    )
    .await;

    let response = test::call_service(&app, test::TestRequest::get().uri("/").to_request()).await;

    assert_eq!(response.status(), 200);
    assert_eq!(test::read_body(response).await, "8080 1");
}

#[actix_web::test]
async fn two_reads_in_one_handler_are_the_same_value() {
    let fixture = Fixture::new(8080, 1);
    fixture.install();

    async fn handler(first: Config<Server>, second: Config<Server>) -> String {
        format!("{}", Arc::ptr_eq(&first.0, &second.0))
    }

    let app = test::init_service(
        App::new()
            .wrap(DynamicConfig::new(sections![Server]))
            .route("/", web::get().to(handler)),
    )
    .await;

    let response = test::call_service(&app, test::TestRequest::get().uri("/").to_request()).await;

    assert_eq!(test::read_body(response).await, "true");
}

#[actix_web::test]
async fn a_request_never_tears_across_a_reload() {
    // The whole reason this crate exists.
    let fixture = Fixture::new(8080, 1);
    fixture.install_moving();

    async fn handler(server: Config<MovingServer>, flags: Config<MovingFeatures>) -> String {
        format!("{} {}", server.port, flags.generation)
    }

    let app = test::init_service(
        App::new()
            .wrap(DynamicConfig::new(sections![MovingServer, MovingFeatures]))
            .route("/", web::get().to(handler)),
    )
    .await;

    let taken = Sections::new()
        .section(MovingServer::try_current)
        .section(MovingFeatures::try_current)
        .take();

    fixture.write(9090, 2);
    fixture.reload_moving();

    assert_eq!(taken.get::<MovingServer>().unwrap().port, 8080);
    assert_eq!(taken.get::<MovingFeatures>().unwrap().generation, 1);

    let response = test::call_service(&app, test::TestRequest::get().uri("/").to_request()).await;

    assert_eq!(test::read_body(response).await, "9090 2");
}

#[actix_web::test]
async fn a_section_the_middleware_was_not_given_says_so() {
    let fixture = Fixture::new(8080, 1);
    fixture.install();

    async fn handler(never: Config<NeverLoaded>) -> String {
        format!("{}", never.unused)
    }

    let app = test::init_service(
        App::new()
            // `NeverLoaded` is deliberately absent.
            .wrap(DynamicConfig::new(sections![Server]))
            .route("/", web::get().to(handler)),
    )
    .await;

    let response = test::call_service(&app, test::TestRequest::get().uri("/").to_request()).await;

    assert_eq!(response.status(), 500);

    let body = test::read_body(response).await;
    let rendered = String::from_utf8_lossy(&body);

    // The client learns the server is misconfigured and nothing else.
    assert!(
        !rendered.contains("NeverLoaded"),
        "a type path reached a client"
    );

    // The log line carries the whole thing.
    let detail = dynamic_config_actix::SnapshotMissing::Section(
        dynamic_config_web_core::NotInScope::NotListed("NeverLoaded"),
    )
    .to_string();

    assert!(
        detail.contains("sections!"),
        "it says how to fix it: {detail}"
    );
}

#[actix_web::test]
async fn a_handler_with_no_middleware_says_that_instead() {
    async fn handler(server: Config<Server>) -> String {
        format!("{}", server.port)
    }

    let app = test::init_service(App::new().route("/", web::get().to(handler))).await;

    let response = test::call_service(&app, test::TestRequest::get().uri("/").to_request()).await;

    assert_eq!(response.status(), 500);

    assert!(
        dynamic_config_actix::SnapshotMissing::NoMiddleware
            .to_string()
            .contains("DynamicConfig"),
        "the log line should name the fix"
    );
}

#[actix_web::test]
async fn no_configuration_value_reaches_the_rejection() {
    let fixture = Fixture::new(8080, 1);
    fixture.install();

    async fn handler(never: Config<NeverLoaded>) -> String {
        format!("{}", never.unused)
    }

    let app = test::init_service(
        App::new()
            .wrap(DynamicConfig::new(sections![Server, Features]))
            .route("/", web::get().to(handler)),
    )
    .await;

    let response = test::call_service(&app, test::TestRequest::get().uri("/").to_request()).await;
    let body = test::read_body(response).await;
    let rendered = String::from_utf8_lossy(&body);

    assert!(!rendered.contains("8080"), "a port reached an error body");
}
