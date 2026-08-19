<div align="center">

# dynamic-config-web

**A request-scoped configuration snapshot for Rust web services: one reading per request, however many sections the handler touches.**

[The book](https://dynamic-config-rs.github.io/serving-http.html) · [The engine](https://github.com/dynamic-config-rs/dynamic-config) · [docs.rs](https://docs.rs/dynamic-config-axum)

</div>

---

```toml
[dependencies]
dynamic-config = { version = "0.6", features = ["json", "watch"] }
dynamic-config-axum = "0.3.0"   # or -actix, or -loco
```

```rust,ignore
use dynamic_config_axum::{Config, SnapshotLayer};
use dynamic_config_web_core::sections;

async fn index(
    Config(server): Config<Server>,
    Config(features): Config<Features>,
) -> String {
    // One reading, taken when the request began. These two cannot be
    // different generations.
    format!("{}:{} cache={}", server.host, server.port, features.cache)
}

let app = Router::new()
    .route("/", get(index))
    .layer(SnapshotLayer::new(sections![Server, Features]));
```

## The problem

`Server::current()` is an atomic load, and its own documentation says what
to do with it:

> Cheap enough to call per request, but call it *once* per request and reuse
> the `Arc`: a reload landing between two calls would otherwise let one
> request observe two configurations.

With one section that is easy to honour. With two it is not, because "the
same generation" is a property of a *pair* of reads that no single call site
can see. A handler that reads `Server` and then `Features` can be split by a
reload landing between them, and the response then mixes two documents.

These crates take the reading once, before the handler runs, and hand the
result to every extractor in it.

## The crates

| Crate | For | MSRV |
|---|---|---|
| [`dynamic-config-tower`](dynamic-config-tower) | any `tower` stack — the layer the axum crate is built on | 1.88 |
| [`dynamic-config-axum`](dynamic-config-axum) | axum 0.8 — a `tower` layer and a `Config<T>` extractor | 1.88 |
| [`dynamic-config-actix`](dynamic-config-actix) | Actix Web 4 — a middleware and a `FromRequest` extractor | 1.88 |
| [`dynamic-config-loco`](dynamic-config-loco) | [Loco](https://loco.rs) — an `Initializer`, over the axum crate | 1.94 (loco's own floor) |
| [`dynamic-config-web-core`](dynamic-config-web-core) | the shared snapshot. You do not depend on this directly | 1.88 |

1.88 is the organisation's one floor since the 0.2.x stabilisation
round; loco alone sits higher because loco does. CI checks each row
against the real toolchain.

## What these crates do not do

They do not load configuration, watch files, or own a `WatchHandle`. That
stays in the startup code that calls `init()` and holds the handles — where
it already is, and where it belongs. Adding the layer changes no line of it.

They also ship no routes: no `/healthz`, no `/metrics`, no diagnostics
endpoints. The engine's `status()`, `check()` and `Exposition` are public and
a handler over them is four lines, which is a smaller thing to write than a
route surface is to adopt.

## Sections that are not static

`sections![Server, Features]` expands to `|| Server::try_current()` for each
name, which is what `#[dynamic_config]` generates. A `Dynamic<T>` instance
works too — register the closure yourself:

```rust,ignore
let sections = Sections::new()
    .section(Server::try_current)
    .section(move || handle.current());
```

What you may build on and find unchanged tomorrow is written down: the [Compatibility Contract](https://dynamic-config-rs.github.io/compatibility.html).

## License

MIT.
