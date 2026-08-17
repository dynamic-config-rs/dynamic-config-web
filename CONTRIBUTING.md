# Contributing

Thanks for taking the time.

## The gate

```sh
just check          # fmt, clippy, test, docs, msrv — what CI runs
just examples       # all three examples, end to end
```

Everything needs a stable toolchain and the four MSRV toolchains:

```sh
rustup toolchain install 1.71 1.80 1.88 1.94
```

No Docker, no network, no services. The whole suite runs offline.

## The tests run twice, and both orders matter

```sh
cargo test --workspace
cargo test --workspace -- --test-threads=1
```

A configuration lives in a process-wide static keyed by `TypeId`, so two
tests that share a section type interfere when cargo runs them in parallel.
Every test that *reloads* declares its own section types for that reason —
copy that pattern rather than reaching for an existing one.

## Both adapters answer the same questions

`dynamic-config-axum/tests/scope.rs` and
`dynamic-config-actix/tests/scope.rs` are deliberately parallel: the same
six cases, in each framework's vocabulary. A case added to one belongs in
the other. If a framework genuinely cannot answer a case, say so in the test
file and in `README.md` rather than leaving a gap.

## Scope

These crates take one reading of configuration per request. They do not load
it, watch it, or serve routes over it — see `AGENTS.md` for why, and for
what a change that starts to need those is a signal of.

## Style

- `cargo fmt` decides formatting. Clippy runs with `-D warnings` at both
  feature extremes.
- Public items carry documentation; `missing_docs` is a warning and CI
  denies warnings.
- A comment explains *why*, where the reason is not visible from the code.
  Code that needs a comment to say *what* it does usually wants rewriting
  instead.
- No configuration value may reach an error message, a `Debug` output or a
  log line. Names and types, never values.

## Pull requests

Work lands on `dev`. Add a `CHANGELOG.md` entry under `## [Unreleased]` for
anything a user would notice.

## Security

Report privately through the repository's security advisory form rather than
in a public issue. See `SECURITY.md`.
