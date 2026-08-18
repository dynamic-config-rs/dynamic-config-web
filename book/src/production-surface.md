# Production Surface

The crates ship no routes, because the engine's pieces are public and a
handler over them is shorter than a route surface is to adopt. These are
the handlers, complete — copy, paste, adjust the types.

## Liveness and readiness

Two questions, not one. `/healthz` says the process is alive and must
not fail on configuration — a process that cannot reload should be taken
out of rotation, not restarted into reading the same broken file.
`/readyz` is where *nothing ever loaded* and *the reloads are failing*
answer 503.

```rust,ignore
use axum::{http::StatusCode, Json};
use serde_json::{json, Value};

async fn healthz() -> StatusCode {
    StatusCode::OK
}

async fn readyz() -> (StatusCode, Json<Value>) {
    let status = ServerConfig::status();

    let code = if status.generation == 0 || !status.is_healthy() {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    };

    (code, Json(json!({
        "generation": status.generation,
        "healthy": status.is_healthy(),
    })))
}
```

## Prometheus metrics

`Exposition` renders the text format with no metrics dependency:

```rust,ignore
use dynamic_config::telemetry::Exposition;

async fn metrics() -> ([(&'static str, &'static str); 1], String) {
    let mut exposition = Exposition::new();
    exposition.add::<ServerConfig>("server");
    exposition.add::<FeaturesConfig>("features");

    (
        [("content-type", "text/plain; version=0.0.4")],
        exposition.render(),
    )
}
```

## Guarded diagnostics

`explain` renders every layer's answer for one dotted path, redacted by
default. Behind a token, constant-time compared:

```rust,ignore
use axum::extract::Path;
use axum::http::HeaderMap;

const TOKEN: &str = env!("CONFIG_TOKEN");

fn allowed(headers: &HeaderMap) -> bool {
    headers
        .get("x-config-token")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|offered| {
            use subtle::ConstantTimeEq;
            offered.as_bytes().ct_eq(TOKEN.as_bytes()).into()
        })
}

async fn explain(headers: HeaderMap, Path(path): Path<String>) -> (StatusCode, String) {
    if !allowed(&headers) {
        // 404, so a scanner learns nothing from the difference.
        return (StatusCode::NOT_FOUND, String::new());
    }

    match ServerConfig::explain(&path) {
        Ok(explanation) => (StatusCode::OK, explanation.redacted().to_string()),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()),
    }
}
```

## Graceful shutdown

The watchers are RAII: hold the handles in `main`, and dropping them on
the way out stops the threads. With axum's `with_graceful_shutdown`,
nothing else is needed — the engine holds no state that needs flushing,
because every install already happened atomically.

```rust,ignore
let shutdown = async {
    tokio::signal::ctrl_c().await.ok();
};

axum::serve(listener, app).with_graceful_shutdown(shutdown).await?;
// _watchers drop here; the threads end.
```

The Python web package ships this whole page as code
(`/healthz`, `/readyz`, `/metrics`, guards, test doors). This book ships
it as recipes instead, on purpose: a Rust service composes these in
minutes from public engine surface, and a crate would freeze choices —
which metrics names, which token header — that are rightly yours.
