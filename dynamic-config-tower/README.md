# dynamic-config-tower

One reading of [`dynamic-config`](https://docs.rs/dynamic-config)
configuration per request, as a plain `tower` layer.

```toml
[dependencies]
dynamic-config-tower = "0.1.0"
```

```rust,ignore
use dynamic_config_tower::{sections, Snapshot, SnapshotLayer};

let layer = SnapshotLayer::new(sections![Server, Features]);
// … any tower stack: tonic, hyper, or a framework of your own.
```

The service takes one `Snapshot` when a request begins and puts it in the
request's extensions; `snapshot.require::<T>()` reads it back out with an
error that names the fix. [`dynamic-config-axum`](https://docs.rs/dynamic-config-axum)
is this layer plus an extractor — an axum application should take that
crate; this one exists for every tower stack that is not axum.

The layer owns no lifecycle: loading, watching and the `WatchHandle` stay
in your `main`, unchanged.

See [the workspace README](../README.md) for the problem this solves.

MIT licensed.
