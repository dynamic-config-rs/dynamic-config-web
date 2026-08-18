# axum

```toml
[dependencies]
dynamic-config-axum = "<version>"
```

Two pieces: `SnapshotLayer` (the tower layer, re-exported from
`dynamic-config-tower`) and `Config<T>` (a `FromRequestParts` extractor).
Request extensions are axum's request scope, so there is no task-local
and nothing to unwind.

```rust,ignore
let app = Router::new()
    .route("/", get(handler))
    .layer(SnapshotLayer::new(sections![Server, Features]));
```

## Layer order is load-bearing

`.layer()` wraps only the routes present when it is called. This
compiles, and answers 500 on every request:

```rust,ignore
let app = Router::new()
    .layer(SnapshotLayer::new(sections![Server]))   // wraps nothing
    .route("/", get(handler));                      // added after
```

The 500's `Display` names this page. Put the layer after the routes.

## Nesting merges

An outer `Router` and a `nest`ed one may each carry a layer. The outer
runs first; the inner one *merges* into what the outer took rather than
replacing it, so a handler under both sees the union — inner wins on a
type both list.

## The escape hatch

`snapshot(&Parts)` answers the request's `&Snapshot` for code that is
not an extractor — another middleware, a guard, a handler taking
`Request` whole. `SnapshotMissing::NoLayer` is its error, and it means
the layer did not run for this route.

## `Dynamic<T>` instances

`sections![A, B]` expands to `try_current`/`generation` closures on the
static slots. An instance-based `Dynamic<T>` registers by hand:

```rust,ignore
let sections = Sections::new()
    .section_with_generation(
        { let handle = handle.clone(); move || Some(handle.current()) },
        move || handle.generation(),
    );
```

Tests: `dynamic-config-axum/tests/scope.rs` asks the eight questions
every adapter answers; `axum_two_sections` is the runnable example.
