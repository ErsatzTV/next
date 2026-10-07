#!/usr/bin/env bash
# fails unless <tag> is a direct successor of <last release> at or above the bump next-version.sh requires
set -euo pipefail

last="${1:?usage: check-bump.sh <last release> <tag> [<rev> [<changelog section>]]}"
tag="${2:?usage: check-bump.sh <last release> <tag> [<rev> [<changelog section>]]}"
rev="${3:-$tag}"
[[ "$tag" =~ ^v([0-9]+\.[0-9]+\.[0-9]+)$ ]] || { echo "tag '$tag' is not vX.Y.Z" >&2; exit 1; }
version="${BASH_REMATCH[1]}"
# at a release commit the entries have moved from [Unreleased] to [X.Y.Z]
section="${4:-$version}"

required="$("$(dirname "$0")/next-version.sh" "$last" "$rev" "$section" | sed -n 's/^version=//p')"

IFS=. read -r major minor patch <<< "${last#v}"
candidates=("$major.$minor.$((patch + 1))" "$major.$((minor + 1)).0" "$((major + 1)).0.0")
direct=""
for candidate in "${candidates[@]}"; do
  [[ "$version" == "$candidate" ]] && direct=1
done
[[ -n "$direct" ]] || { echo "$tag does not directly follow $last (expected one of: ${candidates[*]})" >&2; exit 1; }

lowest="$(printf '%s\n%s\n' "$version" "$required" | sort -V | head -n 1)"
if [[ "$lowest" != "$required" ]]; then
  echo "$tag is below the required v$required" >&2
  exit 1
fi

echo "$tag follows $last (required at least v$required)"
