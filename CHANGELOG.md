# Changelog

All notable changes to the `dynamic-config-web` crates are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Before 1.0, a breaking change bumps the **minor** version and anything else
bumps the patch. Raising a crate's MSRV is breaking.

The four crates share one version: `dynamic-config-web-core` is named
exactly by both adapters and the Loco crate names the axum one exactly,
so they cannot usefully move apart. The engine is
a separate dependency, named with a caret, and releases on its own schedule.

## [Unreleased]

## 0.3.1 — 2026-08-21

### Changed

- **The engine floor is 0.9**, and `serde` / `serde_json` move to
  `1.0.228` / `1.0.149` behind it — the floors the engine's fold
  requires. Nothing in these crates' own surface changed shape.

- **`tower` left `dynamic-config-axum`'s dependencies** for its
  dev-dependencies, where its only users were: the tests and the
  examples name it directly, the library re-exports the layer from
  `dynamic-config-tower` and never names it. A crate that depends on
  this one no longer compiles `tower` on its account.

## 0.3.0 — 2026-08-19

## 0.2.0 — 2026-08-18

### Changed

- **The engine floor is 0.8** — and that bump is why this release is
  0.3.0: the extractors and layers carry engine types in their public
  API, so the engine's breaking release (a `LoadSpec` field, MSRV
  1.88) is breaking here by composition. Nothing in these crates' own
  surface changed shape.

### Added

- **`dynamic-config-tower`.** The snapshot layer as a plain `tower`
  crate — `SnapshotLayer` and `SnapshotService` moved down from the axum
  crate, which now depends on and re-exports them unchanged. Nothing in
  the layer was axum's: it wraps any service over an `http::Request`,
  which is what lets tonic, plain hyper, or any tower stack take one
  reading per request without adopting a framework. A hyper-free
  integration test drives it through `tower::ServiceExt` alone, as the
  proof it stands without axum.

- **`dynamic_config_loco::snapshot` is re-exported.** The crate's claim is
  the axum surface unchanged, and the free function — the door for code
  that is not an extractor — was the one piece missing from it.

## 0.1.0 — 2026-08-17

### Added

- **`dynamic-config-web-core`.** `Sections`, a list of closures read once
  per request into a `Snapshot`; `Snapshot::get` and `Snapshot::require`;
  `NotInScope`, which distinguishes a section that was never registered from
  one that was registered and had not loaded; and the `sections!` macro.

  **`take()` retries rather than assuming.** Each configuration has its own
  atomic cell and the engine keeps no epoch across them, so reading N
  sections is N independent loads — a reload landing between two of them
  would put two generations in one snapshot, which is the exact bug this
  crate exists to prevent. `take()` reads the install counters, reads the
  sections, and reads the counters again, starting over if anything moved.
  `Sections::section` takes a reader alone and cannot supply a counter;
  `is_consistent()` says whether a list can make the check, and `sections!`
  always produces one that can.

  The closures are the design, not a workaround. `#[dynamic_config]` writes
  `try_current()` as an inherent method, so no generic function can call it
  through a trait — and a closure additionally covers a `Dynamic<T>`
  instance, which a trait on the type could not reach. Nothing is added to
  the engine or to the proc-macro, so these crates build against the
  `dynamic-config 0.6` that is already published.

  `Snapshot`'s `Debug` prints section names and never their contents.

- **`dynamic-config-axum`.** `SnapshotLayer`, a `tower` layer that takes one
  reading per request into the request's extensions, and `Config<T>`, a
  `FromRequestParts` extractor that reads it back out. Request extensions
  are axum's request scope, so nothing is thread-local and nothing has to be
  unwound afterwards.

  A handler asking for a section the layer was not given gets `500` naming
  the type and the fix. That is a wiring mistake rather than anything a
  client did, which is why it is not a `4xx`.

- **`dynamic-config-actix`.** The same two pieces through Actix's
  `Transform` and `FromRequest`: `DynamicConfig` and `Config<T>`.

- **`Snapshot::merged_with`**, and both adapters use it: layers nest, an
  outer `Router`/`App` and an inner one may each carry a list, and a bare
  insert would erase what the outer took — leaving a handler under both with
  a 500 telling it to add a section it had already added.

- **`dynamic-config-loco`.** The `Initializer` Loco asks a library for,
  over the axum crate — `DynamicConfig::boxed(sections![..])` in `Hooks`,
  and the layer and extractor are re-exported unchanged. Loco is axum
  underneath, so there is nothing else to write.

  `ctx.config` is left alone, and the crate's documentation says why: Loco
  reads `config/development.yaml` once at boot for the database URL, the
  worker mode and the listen port, all of which should be read once. This
  is for the other half.

- **One test suite, asked of both.** Eight cases each: a handler reads the
  sections it asked for, two reads in one handler are the same value, a
  request never tears across a reload, a snapshot that straddled one is
  refused and retried, a nested layer does not erase the outer one, an
  unregistered section says so, a missing layer says something different,
  and no configuration value reaches a rejection body.

  A rejection body says only that the server is misconfigured. The type path
  it happened to be is in `Display`, for whoever reads the logs — a client
  that asked for a section this application never registered does not need
  the module tree it lives in. `Config<T>`'s own `Debug` prints the type
  name and never the section, for the same reason `Snapshot`'s does.

### Notes

These crates own no lifecycle. They do not load configuration, watch files,
or hold a `WatchHandle` — that stays in the startup code that already does
it. They ship no routes either: `status()`, `check()` and `Exposition` are
public on the engine, and a handler over them is shorter than a route
surface is to adopt.
