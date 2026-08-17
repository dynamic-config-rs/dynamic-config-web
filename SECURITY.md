# Security

## Reporting

Report privately through
[the repository's advisory form](https://github.com/dynamic-config-rs/dynamic-config-web/security/advisories/new)
rather than in a public issue.

Include what you did, what happened, and what you expected. A runnable
reproduction is worth more than a description of one.

## Scope

These crates take one reading of an already-loaded configuration and hand it
to a request. They open no files, no sockets and no processes; every value
they carry was resolved by [`dynamic-config`](https://github.com/dynamic-config-rs/dynamic-config)
before a request began.

That makes the surface small, and these are the properties it holds.

| Property | How it is kept |
|---|---|
| No configuration value reaches an error body | Rejections name the type and the fix; a test asserts no value appears in one |
| No configuration value reaches a log line | `Snapshot`'s `Debug` prints section names only, asserted by a test |
| A request cannot see two generations | The snapshot is taken once, by the middleware, before the handler runs |
| No `unsafe` | `#![forbid(unsafe_code)]` in all four crates, checked in CI |

An issue in how configuration is *resolved* — precedence, secrets,
redaction, the cache on disk — belongs to the engine's repository, which has
its own advisory form and its own threat model.

## What is not a vulnerability here

- **A handler reading `Config::current()` directly and tearing.** These
  crates offer a way not to; they cannot stop code that does not use it.
- **A section that never loaded answering `500`.** That is the designed
  behaviour for a startup-order mistake, and the message says which one.
- **Configuration visible to anything already inside the process.** A
  snapshot is an `Arc` in memory. Process isolation is the boundary; these
  crates do not add one.

## Standing rule

Every open Dependabot or code-scanning alert is triaged before a release
ships: the dependency moves, or the alert is dismissed with a written reason
saying why the vulnerable path is not reachable and what would reopen the
question.
