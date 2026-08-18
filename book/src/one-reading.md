# One Reading per Request

What the snapshot promises, stated exactly — including the two places
where the promise ends, on purpose.

## The promise

`SnapshotLayer` calls `Sections::take()` once, before anything downstream
runs. `take()` reads every section's install counter, reads the sections,
and reads the counters again; if anything moved it starts over, up to
eight times. A snapshot that would have straddled a reload is therefore
refused and retaken — one request, one reading, however many sections
and however many extractors.

Within one request:

- `Config<T>` twice answers the **same `Arc`** — not merely equal.
- Two different sections came from **one read pass** that no install
  interrupted.

## Where the promise ends, honestly

**Two sections at different versions is not a tear.** Each configuration
has its own atomic cell and the engine keeps no epoch across them. If
`server.toml` reloaded at 12:00:00 and `features.toml` at 12:00:02, a
request between those moments correctly sees the new server document
with the old features one — that is the true state of the world, not a
mixed read. What `take()` refuses is a snapshot whose *reads straddled an
install*; it cannot promise cross-file simultaneity that never existed.

**A writer faster than the retry budget wins.** Something reloading so
fast that eight consecutive read passes are all disturbed exhausts the
retry, and the last read is served unchecked — no worse than not
checking, which is what every caller had before these crates. Real
reloads are file events milliseconds apart; the stress test in
`web-core` pins exactly this boundary.

## The errors are part of the design

`NotInScope` has two variants because the two mistakes have different
fixes: `NotListed` means *add the section to `sections![...]`*;
`NotLoaded` means *call `init()` before serving*. The rejection a client
sees names neither a type path nor a value — the detail is in `Display`,
for whoever reads the logs.
