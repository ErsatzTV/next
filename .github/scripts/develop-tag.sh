#!/usr/bin/env bash
# develop builds are prereleases of the version the next release will get, so they sort below it
set -euo pipefail

root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
sha="$(git -C "$root" rev-parse --verify "${1:?usage: develop-tag.sh <sha>}^{commit}")"
last_release=$(git -C "$root" describe --tags --abbrev=0 --match 'v[0-9]*' --exclude '*-develop' "$sha")
version="$("$root/.github/scripts/next-version.sh" "$last_release" "$sha" | sed -n 's/^version=//p')"
echo "last_release=${last_release}"
echo "tag=v${version}-${sha:0:8}-develop"
