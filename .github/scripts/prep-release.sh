#!/usr/bin/env bash
# moves [Unreleased] into a dated [X.Y.Z] section, updates the compare links and sets the workspace version
set -euo pipefail

root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
scripts="$root/.github/scripts"
changelog="$root/CHANGELOG.md"
repo_url="https://github.com/${GITHUB_REPOSITORY:-ErsatzTV/next}"
files=(CHANGELOG.md Cargo.toml Cargo.lock)

# next-version.sh reads the committed changelog, so uncommitted entries would be ignored
if ! git -C "$root" diff --quiet HEAD -- "${files[@]}"; then
  echo "commit or stash changes to ${files[*]} first" >&2
  exit 1
fi

last_release="$(git -C "$root" describe --tags --abbrev=0 --match 'v[0-9]*' --exclude '*-develop' HEAD)"
required="$("$scripts/next-version.sh" "$last_release" HEAD | sed -n 's/^version=//p')"
version="${1:-$required}"
version="${version#v}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "usage: prep-release.sh [X.Y.Z]" >&2; exit 1; }
"$scripts/check-bump.sh" "$last_release" "v$version" HEAD Unreleased

if grep -q "^## \[$version\]" "$changelog"; then
  echo "CHANGELOG.md already has a '## [$version]' section" >&2
  exit 1
fi

previous="$(sed -n "s#^\[Unreleased\]: $repo_url/compare/\(v[0-9.]*\)\.\.\.HEAD\$#\1#p" "$changelog")"
[[ -n "$previous" ]] || { echo "CHANGELOG.md has no '[Unreleased]: $repo_url/compare/vX.Y.Z...HEAD' link" >&2; exit 1; }
if [[ "$previous" != "$last_release" ]]; then
  echo "[Unreleased] compares from $previous but the last release tag is $last_release" >&2
  exit 1
fi

out="$(mktemp)"
trap 'rm -f "$out"' EXIT
awk -v version="$version" -v date="$(date +%F)" -v previous="$previous" -v url="$repo_url" '
  { line = $0; sub(/^\xef\xbb\xbf/, "", line) }
  line == "## [Unreleased]" { print; print ""; print "## [" version "] - " date; next }
  line ~ /^\[Unreleased\]: / {
    print "[Unreleased]: " url "/compare/v" version "...HEAD"
    print "[" version "]: " url "/compare/" previous "...v" version
    next
  }
  { print }
' "$changelog" > "$out"
cp "$out" "$changelog"

awk -v version="$version" '
  /^\[/ { in_package = ($0 == "[workspace.package]") }
  in_package && /^version = / { print "version = \"" version "\""; found = 1; next }
  { print }
  END { if (!found) exit 3 }
' "$root/Cargo.toml" > "$out" || { echo "Cargo.toml has no version in [workspace.package]" >&2; exit 1; }
cp "$out" "$root/Cargo.toml"
(cd "$root" && cargo update --workspace --offline --quiet)

mismatched="$(cd "$root" && cargo metadata --locked --offline --no-deps --format-version 1 \
  | jq -r --arg v "$version" '.packages[] | select(.version != $v) | "\(.name) \(.version)"')"
[[ -z "$mismatched" ]] || { echo "crates not at $version:"$'\n'"$mismatched" >&2; exit 1; }

# same check release.yml preflight runs
CHANGELOG="$changelog" "$scripts/notes.sh" "v$version" > /dev/null

echo "Prepared v$version (compare from $previous). Next:"
echo "  git commit -m 'chore: prep for release v$version' ${files[*]} && git push origin main"
echo "  git tag v$version && git push origin v$version"
