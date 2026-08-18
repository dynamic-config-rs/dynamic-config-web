# Actix Web

```toml
[dependencies]
dynamic-config-actix = "<version>"
```

The same two pieces through Actix's seams: `DynamicConfig` is a
`Transform` middleware, `Config<T>` a `FromRequest` extractor.

```rust,ignore
App::new()
    .wrap(DynamicConfig::new(sections![Server, Features]))
    .service(handler);
```

## Where Actix differs from axum

- **`wrap` order reads outside-in** — the last `wrap` runs first. The
  snapshot middleware can sit anywhere; it only needs to run before the
  handler.
- **Extensions live behind a `RefCell`**, so the free function
  `snapshot(&HttpRequest)` answers a **clone** of the snapshot rather
  than a reference — cheap (it is a map of `Arc`s), and the reason its
  signature differs from axum's.
- **Scoped services nest** exactly as axum's routers do: an outer `wrap`
  and a `web::scope(...).wrap(...)` merge, inner wins per type.
- The rejection type implements `ResponseError`; the body a client sees
  is generic on purpose, and the type path stays in `Display` for logs.

Workers each run their own copy of the app factory; the sections list is
behind an `Arc`, so N workers share one list and each request anywhere
takes its own snapshot.

Tests: `dynamic-config-actix/tests/scope.rs` — kept parallel to the
axum file, case for case; `actix_two_sections` is the runnable example.
