# Releasing

Four crates on one version, published to crates.io in dependency order.

```text
dynamic-config-web-core     first, always
  ├── dynamic-config-axum
  │     └── dynamic-config-loco
  └── dynamic-config-actix
```

Every adapter names `dynamic-config-web-core` with an exact requirement —
and the Loco crate names the axum one the same way — so the four move
together and crates.io will not accept an adapter before the
core it names exists.

The engine is a different matter: it is named with a caret
(`dynamic-config = "0.6"`) and released from its own repository on its own
schedule. Nothing here waits for it.

## The branch model

Work lands on `dev`. `main` is production: it accepts no direct pushes — not
even from admins — only pull requests whose gates ("CI is green", "Security
is green") have passed, merged with a linear history.

**Merging a version bump into `main` is the release.** There is no tag to
push by hand: `release.yml` runs on every push to `main`, checks whether the
version is new, and — only then — verifies, publishes, and mints the tag and
the GitHub release at the merge commit.

## The lifecycle, step by step

1. **Land the work on `dev`** through pull requests, entries accumulating
   under `## [Unreleased]` in `CHANGELOG.md`.
2. **Pre-flight.** `just check` on `dev` — formatting, clippy at both
   feature extremes, the suite in both thread orders, the documentation
   build, and each crate against its own MSRV. `just examples` too: an
   example that only compiles is not an example.
3. **`cargo release patch --execute`** (or `minor`; pre-1.0 a breaking
   change is `minor`). It bumps all four crates, rotates the changelog and
   makes one commit — no push, no tag, no publish.

   **The first release is `cargo release 0.1.0 --execute`.** The manifests
   already say 0.1.0 and nothing is published, so there is nothing to bump —
   naming the version explicitly rotates the changelog and leaves the
   version where it is, which is what `release.yml` then sees as new.
4. **Read the commit.** `git show --stat HEAD`: four manifests, one
   changelog, four READMEs whose snippets moved with them.
5. **`./scripts/promote.sh`.** Pushes `dev`, opens or updates the pull
   request, arms auto-merge and waits; when both gates pass, the
   squash-merge lands — **that merge is the release**.
6. **`./scripts/watch-release.sh`.** Follows the run: verify, then publish
   in the order above with a pause between waves, then the tag and the
   GitHub release.

## What an operator has to have ready

`CARGO_REGISTRY_TOKEN` as a secret on the `crates-io` environment, with
publish rights to all three names. Nothing else.

## Afterwards

```sh
cargo add dynamic-config-axum   # in a scratch project
cargo build
```

The first release has one thing to check that later ones do not: that
`dynamic-config-web-core` is on crates.io *before* the adapters, since
`cargo publish` will refuse an adapter whose exact dependency does not
resolve. The waves in `release.yml` are what arrange it, and the 45-second
pause is for the index to catch up.

## Version policy

- **Pre-1.0, a breaking change bumps the minor version** and everything else
  the patch.
- **Raising a crate's MSRV is breaking.** The four floors are 1.71, 1.80,
  1.88 and 1.94 — they differ because each crate pays for what it pulls in.
  Each is measured against a
  lockfile resolved by stable, which is what a user's `cargo add` produces —
  so an upstream release can move a floor without a line changing here. When
  it does, that is a minor bump with a changelog entry, not a quiet edit.
- Adding a framework is additive. Removing one is breaking.
- The engine floor moving is not by itself breaking: what matters is whether
  *these* crates' surface moved.
