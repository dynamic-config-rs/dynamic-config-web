# Stability & Versioning

Five crates, one version, published together: each names the one below
it exactly (`=x.y.z`), so they cannot drift apart. The engine is named
with a caret and releases on its own schedule; nothing here waits for
it.

**Beta**, like the rest of the organisation: the surface is small on
purpose and has not needed to move, but pre-1.0 a breaking change bumps
the minor version and the changelog says so in its first line.

- **Raising a crate's MSRV is breaking.** The floors differ — 1.71,
  1.71, 1.80, 1.88, 1.94 — because each crate pays only for what it
  pulls in, and each is measured against a lockfile resolved by stable,
  which is what a user's `cargo add` produces.
- **Adding a framework crate is additive. Removing one is breaking.**
- The engine floor moving is not by itself breaking here: what matters
  is whether *these* crates' surface moved.

What will not be added, so nobody waits for it: a `Wiring` lifecycle,
mounted routes, health endpoints. The reasoning is on the
[Introduction](introduction.md), and it is a charter, not a backlog.
