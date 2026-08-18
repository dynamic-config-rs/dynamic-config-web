# Long-lived Connections

A WebSocket upgrade, an SSE route and a streaming body all *begin* as an
HTTP request, so the layer gives each one a snapshot — and for the
handshake that is correct: whether to accept, and from which
configuration, is a request-scoped question.

What the snapshot must not become is the connection's configuration for
life.

```rust,ignore
async fn socket(upgrade: WebSocketUpgrade, Config(server): Config<ServerConfig>) -> Response {
    // `server` is right, HERE: the handshake's decisions are one reading.
    if !server.websockets_enabled() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }

    upgrade.on_upgrade(|mut socket| async move {
        // Do NOT move `server` in here as "the config". The connection
        // may live for an hour; read fresh state where you use it:
        while let Some(message) = socket.recv().await {
            let limits = ServerConfig::current();   // per message batch
            // …
        }
    })
}
```

The rule, stated once: **the snapshot is the handshake's; inside the
connection loop, `T::current()` per iteration or per message batch.** If
the protocol wants a push on change, the engine's `changes()` stream is
the event source — subscribe in the connection task and forward.

The Python package documents the same boundary from the other side (ASGI
gives a `websocket` scope no request scope at all); the two books
describe one behaviour. The extra care here is because in axum the temptation
compiles: an `Arc<T>` moves into `on_upgrade` without complaint, and
nothing warns that it will be stale by lunchtime.
