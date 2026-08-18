# dynamic-config-actix

A request-scoped [`dynamic-config`](https://docs.rs/dynamic-config) snapshot
for Actix Web: one middleware, one extractor.

```toml
[dependencies]
dynamic-config-actix = "0.2.0"
```

```rust,ignore
use dynamic_config_actix::{Config, DynamicConfig};
use dynamic_config_web_core::sections;

#[get("/")]
async fn index(server: Config<Server>, features: Config<Features>) -> String {
    format!("{} {}", server.port, features.cache)
}

App::new()
    .wrap(DynamicConfig::new(sections![Server, Features]))
    .service(index);
```

Both sections come from one reading taken when the request began, so a
reload landing mid-request cannot show one response two generations.

The middleware owns no lifecycle: loading, watching and the `WatchHandle` stay in
your `main`, unchanged.

See [the workspace README](../README.md) for the problem this solves, and
`cargo run -p dynamic-config-actix --example actix_two_sections` for a
runnable demonstration.

MIT licensed.
