# Working in this repository

Three crates that give a Rust web service one reading of its configuration
per request.

```text
dynamic-config-web-core/   the snapshot, and nothing framework-shaped
  src/lib.rs               Snapshot, Sections, NotInScope, sections!
dynamic-config-axum/       a tower Layer and a FromRequestParts extractor
dynamic-config-actix/      a Transform middleware and a FromRequest extractor
```

## What these crates are

A `Sections` list of closures, read once when a request begins, into a
`Snapshot` the framework puts where the request can reach it. Every read in
the handler comes back out of that one value.

## What they are not, and must not become

- **They own no lifecycle.** No `init()`, no `watch()`, no `WatchHandle`.
  That stays in the application's `main`, where it already is. A pull
  request that adds a `Wiring` type here is changing what these crates are.
- **They ship no routes.** No `/healthz`, no `/metrics`, no diagnostics. The
  engine's `status()`, `check()` and `Exposition` are public, and a handler
  over them is shorter than a route surface is to adopt. The Python package
  makes the opposite choice for a reason that does not apply here: a Python
  service has no `tower` and no `Router` to compose with.
- **They add nothing to the engine.** The closure API exists precisely so
  that `dynamic-config` and its proc-macro do not have to change. If a
  change here starts to need an engine change, that is the signal to stop
  and ask, not to make it.

## The rules

1. **One reading per request.** A handler that extracts the same section
   twice gets the same `Arc`. A handler that extracts two sections gets one
   generation. That is the whole product.
2. **A snapshot is not a cache.** The next request reads again.
3. **No configuration value reaches an error body.** A rejection names the
   *type* and the fix, never a value — `Snapshot`'s `Debug` follows the same
   rule, because a request struct ends up in log lines.
4. **A missing section says which mistake it was.** `NotListed` and
   `NotLoaded` have different fixes, so they are different variants.

## What must move together

Both adapters answer one set of questions. Adding a case to
`dynamic-config-axum/tests/scope.rs` means adding it to
`dynamic-config-actix/tests/scope.rs` — the two files are deliberately
parallel, and a case only one of them answers is a promise only half kept.

Adding or changing public API means: the crate, both test files, both
examples, `CHANGELOG.md`, and the READMEs whose snippets
`scripts/sync-readme-versions.sh` counts (`expected = 3`).

## The gate

```sh
just check          # fmt, clippy, test (both thread orders), docs, msrv
just examples
```

## Things that have already bitten

- **A configuration lives in a process-wide static keyed by `TypeId`.** Two
  tests sharing a section type race when cargo runs them in parallel: the
  reloading test moved `Server` under a test that only read it, and it
  passed alone. Every test that *reloads* declares its own section types —
  that is what `MovingServer` and `MovingFeatures` are for.
- **actix-web's declared MSRV is not this crate's code.** 4.14 requires
  1.88 and so do the `icu_*` crates `url` pulls in, which is why the
  manifest says 1.88 even though the adapter itself compiles on far less.
  The number that belongs there is what a fresh `cargo add` needs.
- **`Sections::take()` must be called by the middleware, never by the
  extractor.** An extractor that took its own snapshot would put the tear
  back exactly where it was.

## What this repository never does

Commit, push, tag or publish on its own. `scripts/` prepares; a person runs
it.
