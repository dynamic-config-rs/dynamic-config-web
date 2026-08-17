# Everything CI runs, in the order that fails fastest.
#
# Four crates: the request scope, one translation of it per framework, and
# Loco — which is axum underneath.
# No containers, no network, no Python — the whole gate is cargo.

default: check

# The whole gate, locally.
check: fmt lint test docs msrv

# Formatting, as CI checks it.
fmt:
    cargo fmt --all -- --check

# Clippy at both feature extremes, warnings denied.
lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo clippy --workspace --all-targets --no-default-features -- -D warnings

# The suite, then again single-threaded. A configuration lives in a
# process-wide static keyed by TypeId, so two tests sharing a section type
# race in parallel and pass alone — both orders have to be green.
test:
    cargo test --workspace
    cargo test --workspace -- --test-threads=1

# The three runnable examples. An example that only compiles is not an
# example.
examples:
    cargo run -p dynamic-config-axum --example two_sections
    cargo run -p dynamic-config-actix --example two_sections
    cargo run -p dynamic-config-loco --example two_sections

# Documentation, with warnings denied — a broken intra-doc link is a broken
# link on docs.rs.
docs:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features

# Each crate against its declared floor. A floor nobody compiles against is
# a number in a manifest.
msrv:
    cargo +stable generate-lockfile
    cargo +1.71 check -p dynamic-config-web-core --locked
    cargo +1.80 check -p dynamic-config-axum --locked
    cargo +1.88 check -p dynamic-config-actix --locked
    cargo +1.94 check -p dynamic-config-loco --locked

# What the dependency graph resolves to, for the advisory scan.
audit:
    cargo deny check advisories
