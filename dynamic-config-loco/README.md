# dynamic-config-loco

A request-scoped [`dynamic-config`](https://docs.rs/dynamic-config) snapshot
for [Loco](https://loco.rs): one initializer, one extractor.

```toml
[dependencies]
dynamic-config-loco = "0.1.0"
```

```rust,ignore
use dynamic_config_loco::{sections, Config, DynamicConfig};
use loco_rs::prelude::*;

impl Hooks for App {
    async fn initializers(_ctx: &AppContext) -> Result<Vec<Box<dyn Initializer>>> {
        Ok(vec![DynamicConfig::boxed(sections![Server, Features])])
    }
}

async fn index(
    Config(server): Config<Server>,
    Config(features): Config<Features>,
) -> Result<Response> {
    format::text(&format!("{} {}", server.port, features.cache))
}
```

Both sections come from one reading taken when the request began, so a
reload landing mid-request cannot show one response two generations.

## Loco's own configuration is a different thing

`config/development.yaml` is where Loco's database URL, worker mode and
server port live. None of it reloads, and none of it should — Loco binds
its listener and builds its pool from those values once.

This crate is for the other half: the settings an operator changes while
the service runs. Keep them in their own file, with their own
`#[dynamic_config]` sections, and leave `ctx.config` to Loco.

## What it is

Loco is axum underneath, so the layer and the extractor are
[`dynamic-config-axum`](https://docs.rs/dynamic-config-axum)'s, re-exported
unchanged. What this adds is the `Initializer` Loco asks a library for.

Writing that yourself is three lines in your own initializer, and is fine.

See [the workspace README](../README.md) for the problem all of this
solves, and `cargo run -p dynamic-config-loco --example two_sections` for a
runnable demonstration — the initializer driven through `after_routes` the
way Loco's own boot sequence drives it.

MIT licensed.
