# Loco

```toml
[dependencies]
dynamic-config-loco = "<version>"
```

Loco is axum underneath, so the layer and the extractor are the axum
crate's, re-exported unchanged. What Loco adds is a *place to register
one* — the `Initializer` trait — and that registration is the whole
crate:

```rust,ignore
use dynamic_config_loco::{sections, DynamicConfig};

impl Hooks for App {
    async fn initializers(_ctx: &AppContext) -> Result<Vec<Box<dyn Initializer>>> {
        Ok(vec![DynamicConfig::boxed(sections![Server, Features])])
    }
}
```

Loco calls `after_routes` once with the router it finished building, so
every route the application declares is covered — including the ones
Loco adds itself.

## Loco's own configuration is a different thing

`config/development.yaml` is where the database URL, the worker mode and
the listen port live. None of it reloads, and none of it should: Loco
binds its listener and builds its pool from those values once. This
crate is for the *other* half — what an operator turns while the service
runs. Keep those settings in their own file with their own
`#[dynamic_config]` sections, and leave `ctx.config` to Loco.

## Where loading goes

`Hooks::boot`, before the router exists — the same rule as everywhere
else: the crate owns no lifecycle.

Tests drive the real `after_routes` with a real `AppContext` (via Loco's
own `tests_cfg`); `loco_two_sections` is the runnable example, and its
`main` is a faithful copy of what `loco_rs::boot` runs.
