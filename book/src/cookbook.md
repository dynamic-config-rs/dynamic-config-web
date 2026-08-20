# Cookbook: axum + Vault + Prometheus, End to End

The whole service, once — load from Vault with a file fallback, serve
with one reading per request, expose the metrics and readiness the
[Production Surface](production-surface.md) defines, and shut down
without dropping a request. Every piece is documented alone elsewhere;
what a production service needs is all of them at once, wired in the
right order.

The order **is** the recipe:

```text
1. sources         file first, Vault over it
2. init            fail HERE, not on the first request
3. watch + poll    file watcher, Vault version poller
4. routes          /healthz /readyz /metrics, then the app, then the layer
5. serve           with graceful shutdown holding the watcher handles
```

```toml
[dependencies]
dynamic-config = { version = "0.7", features = ["json", "toml", "watch"] }
dynamic-config-axum = "0.2"
dynamic-config-vault = "0.7"
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

```rust,ignore
use std::time::Duration;

use axum::routing::get;
use axum::{Json, Router};
use dynamic_config::telemetry::Exposition;
use dynamic_config::{dynamic_config, RemoteSource, RemoteWatch};
use dynamic_config_axum::{Config, SnapshotLayer};
use dynamic_config_vault::Vault;
use serde::Deserialize;
use serde_json::json;

#[dynamic_config]
#[derive(Debug, Deserialize)]
struct App {
    listen: String,
    pool_size: u32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Sources. The file carries the shape and the local defaults;
    //    Vault carries what operations turns. The remote layer outranks
    //    the file where both speak — `explain("pool_size")` will say so.
    //    A real deployment logs in (`Auth::kubernetes(..)`) instead of
    //    holding a token; the dev server's token is for the dev server.
    App::set_remote(
        Vault::new(&vault_addr(), "secret", "myapp/app")
            .with_key("app")
            .with_token(&std::env::var("VAULT_TOKEN")?),
    );
    App::refresh_remote()?;

    // 2. Init — the fail-fast line. A typo in the file or a sealed
    //    Vault stops the deploy here, where the orchestrator retries,
    //    not on the first request, where a user pays for it.
    App::builder("app")
        .file("/etc/myapp/config.toml")
        .env("MYAPP_")
        .init()?;

    // 3. Watch the file; poll Vault's *metadata version* — the secret
    //    is read only when the version moves, so an unchanged secret
    //    costs no transfer, no decrypt, no audit line. The sink is the
    //    same reload path a file edit takes: validation, hooks, LKG.
    let _watcher = App::builder("app")
        .file("/etc/myapp/config.toml")
        .env("MYAPP_")
        .watch(Duration::from_millis(500))?;

    let sink = App::remote_sink();
    let vault_watch = RemoteWatch::new();
    let watching = vault_watch.watching();
    let poller = std::thread::spawn({
        let watcher = Vault::new(&vault_addr(), "secret", "myapp/app")
            .with_key("app")
            .with_token(&std::env::var("VAULT_TOKEN")?)
            .reporting_to(sink);

        move || watcher.watch(&watching, Duration::from_secs(30), move |doc| sink.apply(doc))
    });

    // 4. Routes. The three operational routes come FIRST and take no
    //    snapshot layer: a readiness probe must answer even while the
    //    app's own configuration is mid-reload. Then the app, then the
    //    layer — a tower layer wraps only what was there when it was
    //    added.
    let app = Router::new()
        .route("/", get(index))
        .layer(SnapshotLayer::new(dynamic_config_axum::sections![App]));

    let service = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .merge(app);

    // 5. Serve, and shut down without dropping a request. When the
    //    signal lands: stop accepting, drain in-flight, and only then
    //    tear down the pollers — configuration stays live for every
    //    request still draining, because the handles outlive the serve.
    let listener = tokio::net::TcpListener::bind(&App::current().listen).await?;

    axum::serve(listener, service)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;

    // The drain is over; now the Vault loop may stop (a quarter second,
    // whatever the interval), and the file watcher drops with `main`.
    vault_watch.stop();
    poller.join().expect("the watch thread ends")?;

    Ok(())
}

async fn index(Config(app): Config<App>) -> Json<serde_json::Value> {
    Json(json!({ "pool": app.pool_size }))
}

/// LKG serving means READY — a failing reload is a `degraded` detail,
/// never a 503, because restarting into the same broken document turns
/// a degraded service into an outage. The contract is the engine book's
/// [Readiness & Liveness](https://dynamic-config-rs.github.io/readiness.html).
async fn readyz() -> (axum::http::StatusCode, Json<serde_json::Value>) {
    let status = App::status();
    let serving = App::try_current().is_some();

    let code = if serving {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    };

    (
        code,
        Json(json!({
            "status": if !serving { "unready" }
                      else if status.consecutive_failures > 0 { "degraded" }
                      else { "ready" },
            "generation": status.generation,
            "consecutive_failures": status.consecutive_failures,
        })),
    )
}

/// The engine's own counters in Prometheus text format, no metrics
/// crate involved. The names are the
/// [Metrics Contract](https://dynamic-config-rs.github.io/metrics-contract.html)'s.
async fn metrics() -> ([(&'static str, &'static str); 1], String) {
    let mut exposition = Exposition::new();
    exposition.add::<App>("app");

    (
        [("content-type", "text/plain; version=0.0.4")],
        exposition.render(),
    )
}

fn vault_addr() -> String {
    std::env::var("VAULT_ADDR").unwrap_or_else(|_| "http://127.0.0.1:8200".to_owned())
}
```

## The five mistakes this layout is arranged against

1. **Init inside a handler** — the first request pays for the first
   load, and a broken file 503s users instead of failing the deploy.
2. **The snapshot layer around `/readyz`** — a probe that needs the
   app's configuration to answer cannot report the configuration being
   broken.
3. **Watcher handles dropped early** — `let _ = builder.watch(..)`
   drops the handle *on that line* and the watcher with it. Bind it.
4. **Vault polled for the secret** — poll the metadata version; an
   unchanged secret should cost no read, no decrypt, no audit line.
5. **Shutdown that races the drain** — configuration must outlive the
   last in-flight request, which the layout gives you for free: the
   serve returns only after the drain, and the watch teardown comes
   after the serve.
