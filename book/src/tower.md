# Plain tower

```toml
[dependencies]
dynamic-config-tower = "<version>"
```

For every tower stack that is not axum: tonic, plain hyper, a framework
of your own. `SnapshotLayer` wraps any `Service<http::Request<B>>`; the
snapshot goes into the request's extensions, and reading it back out is
yours:

```rust,ignore
use dynamic_config_tower::{sections, Snapshot, SnapshotLayer};
use tower::{service_fn, Layer, Service, ServiceExt};

let service = service_fn(|request: http::Request<Body>| async move {
    let snapshot = request.extensions().get::<Snapshot>().expect("the layer ran");
    let server = snapshot.require::<ServerConfig>()?;   // errors name the fix
    // …
});

let mut wired = SnapshotLayer::new(sections![ServerConfig]).layer(service);
```

`Snapshot::require` is the read that distinguishes the two mistakes:
`NotListed` (add it to `sections![...]`) and `NotLoaded` (call `init()`
before serving). `get` is the `Option` form for code with its own
opinion about absence.

With tonic, attach the layer through `Server::builder().layer(...)`; a
gRPC method reads the snapshot from the request extensions exactly as
above. The [Long-lived Connections](long-lived.md) page applies to
streaming RPCs verbatim.
