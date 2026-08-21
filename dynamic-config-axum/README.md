# dynamic-config-axum

A request-scoped [`dynamic-config`](https://docs.rs/dynamic-config) snapshot
for axum: one `tower` layer, one extractor.

```toml
[dependencies]
dynamic-config-axum = "0.3.1"
```

```rust,ignore
use dynamic_config_axum::{Config, SnapshotLayer};
use dynamic_config_web_core::sections;

async fn index(
    Config(server): Config<Server>,
    Config(features): Config<Features>,
) -> String {
    format!("{} {}", server.port, features.cache)
}

let app = Router::new()
    .route("/", get(index))
    .layer(SnapshotLayer::new(sections![Server, Features]));
```

Both sections come from one reading taken when the request began, so a
reload landing mid-request cannot show one response two generations.

The layer owns no lifecycle: loading, watching and the `WatchHandle` stay in
your `main`, unchanged.

See [the workspace README](../README.md) for the problem this solves, and
`cargo run -p dynamic-config-axum --example axum_two_sections` for a
runnable demonstration.

MIT licensed.
