#!/usr/bin/env bash
# Rotates CHANGELOG.md: the `Unreleased` section gains the new version's
# dated heading.
#
# One changelog for four crates, because they are one product on one
# version — so there is nothing for cargo-release's own
# `pre-release-replacements` to do. Those are applied per *package*, and
# would either look for four changelogs that do not exist or rewrite this
# one four times.
#
# Called from the pre-release hook, which runs once per package. The
# version check makes every run after the first a no-op.
set -euo pipefail
cd "$(dirname "$0")/.."

# Set by cargo-release for its hooks; running this outside a release should
# fail loudly rather than rotate to nothing.
version="${NEW_VERSION:?NEW_VERSION is set by the cargo-release hook environment}"

# `cargo release <level>` without `--execute` is a dry run, and it still
# runs the hooks — with DRY_RUN=true. A look-before-you-leap run must not
# leave the tree dirty.
if [ "${DRY_RUN:-false}" = "true" ]; then
  echo "dry run: would rotate CHANGELOG.md for $version"
  exit 0
fi

if grep -q "^## $version " CHANGELOG.md; then
  exit 0
fi

python3 - "$version" <<'PY'
import datetime
import pathlib
import sys

version = sys.argv[1]
path = pathlib.Path("CHANGELOG.md")
text = path.read_text()

# The template comment spells its heading `[_Unreleased_]` precisely so
# that this search matches only the real one.
heading = "## [Unreleased]"
assert text.count(heading) == 1, "expected exactly one real Unreleased heading"

# Unbracketed, and with no link definition to follow it: this repository's
# tags are its own versions, and `release.yml` greps for exactly this shape
# before it publishes.
today = datetime.date.today().isoformat()
path.write_text(text.replace(heading, f"{heading}\n\n## {version} — {today}", 1))
PY

echo "rotated CHANGELOG.md for $version"
