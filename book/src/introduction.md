# One reading per request

The engine's generated `current()` carries one warning: *call it once per
request and reuse the value — a reload landing between two calls would
otherwise let one request observe two configurations.* With one section
that is easy advice. With two it is impossible advice, because "the same
generation" is a property of a *pair* of reads that no single call site
can see:

```rust,ignore
async fn handler() -> String {
    let server = ServerConfig::current();     // generation 7
    // a reload lands here
    let features = FeaturesConfig::current(); // generation 8
    // this response now mixes two documents
}
```

Both reads are correct. The response is not.

These five crates turn the advice into something the type system
arranges: a layer takes one snapshot when the request begins, and every
extractor in the handler reads out of that snapshot.

```toml
[dependencies]
dynamic-config-axum = "<version>"
```

```rust,ignore
async fn handler(
    Config(server): Config<ServerConfig>,
    Config(features): Config<FeaturesConfig>,
) -> String {
    // One reading. These came from one snapshot.
    format!("{} {}", server.port(), features.cache())
}
```

## The five crates

| Crate | What it is | MSRV |
|---|---|---|
| `dynamic-config-web-core` | the snapshot and the section list — no framework | 1.88 |
| `dynamic-config-tower` | the layer/service pair over any `tower` stack | 1.88 |
| `dynamic-config-axum` | the tower layer re-exported + a `FromRequestParts` extractor | 1.88 |
| `dynamic-config-actix` | the same two pieces through Actix's `Transform`/`FromRequest` | 1.88 |
| `dynamic-config-loco` | the `Initializer` Loco asks a library for, over the axum crate | 1.94 |

## What they are not

**They own no lifecycle.** Loading, watching and the `WatchHandle` stay
in your `main`, exactly where the engine's book puts them. **They ship no
routes**: `status()`, `check()` and `Exposition` are public engine
surface, and [Production Surface](production-surface.md) shows that a
handler over them is shorter than a route surface would be to adopt.
**They add nothing to the engine** — everything here is closures over
API that already exists.

The engine, the stores, and the Python and Node ecosystems each have
their own book — [the family page](https://dynamic-config-rs.github.io/family.html)
is the map. The Python *web* package makes the opposite choice to this
one (it ships routes and a lifecycle), for a reason that does not apply
here: a Python service has no `tower` to compose with.
