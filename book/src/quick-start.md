# Quick Start

```toml
[dependencies]
dynamic-config = { version = "<version>", features = ["toml", "watch"] }
dynamic-config-axum = "<version>"
```

```rust,ignore
use std::time::Duration;

use axum::{routing::get, Router};
use dynamic_config::dynamic_config;
use dynamic_config_axum::{sections, Config, SnapshotLayer};
use serde::Deserialize;

#[dynamic_config]
#[derive(Deserialize)]
struct Server {
    host: String,
    port: u16,
}

#[dynamic_config]
#[derive(Deserialize)]
struct Features {
    cache: bool,
}

async fn index(
    Config(server): Config<Server>,
    Config(features): Config<Features>,
) -> String {
    format!("{}:{} cache={}", server.host, server.port, features.cache)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The lifecycle is yours, unchanged: load before serving, watch after.
    Server::builder("server").file("config.toml").init()?;
    Features::builder("features").file("config.toml").init()?;

    let _watchers = [
        Server::builder("server")
            .file("config.toml")
            .watch(Duration::from_millis(250))?,
        Features::builder("features")
            .file("config.toml")
            .watch(Duration::from_millis(250))?,
    ];

    let app = Router::new()
        .route("/", get(index))
        // After the routes it covers, like any tower layer.
        .layer(SnapshotLayer::new(sections![Server, Features]));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}
```

Edit `config.toml` while it serves. The *next* request answers with the
new document; no request ever mixes two. That is the whole product —
[One Reading per Request](one-reading.md) is what it promises precisely,
and each framework chapter is the same wiring through different seams.

Two mistakes the crates catch for you, loudly:

- A handler asking for a section the layer was not given gets a **500
  naming the type and the fix** — a wiring bug, not a client error.
- `.layer()` *before* `.route()` compiles and 500s on every request —
  the axum chapter says why, and the error text points here.

Runnable versions of exactly this: `cargo run -p dynamic-config-axum
--example axum_two_sections`, and siblings for Actix and Loco.
