#!/usr/bin/env bash
# bump level and next release version, from a changelog section and the schema versions at <rev>
set -euo pipefail

last="${1:?usage: next-version.sh <last release> <rev> [<changelog section>]}"
rev="${2:?usage: next-version.sh <last release> <rev> [<changelog section>]}"
section="${3:-Unreleased}"
root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
scripts="$root/.github/scripts"

[[ "$last" =~ ^v([0-9]+)\.([0-9]+)\.([0-9]+)$ ]] || { echo "last release '$last' is not vX.Y.Z" >&2; exit 1; }
major="${BASH_REMATCH[1]}" minor="${BASH_REMATCH[2]}" patch="${BASH_REMATCH[3]}"

# a missing file or $id reads as 0.0.0, like the loaders
schema_version() {
  local spec="$1:schema/$2.json" id
  git -C "$root" cat-file -e "$spec" 2>/dev/null || { echo 0.0.0; return; }
  id="$(git -C "$root" show "$spec" | jq -r '."$id" // ""')"
  [[ -z "$id" ]] && { echo 0.0.0; return; }
  [[ "$id" =~ /version/([0-9]+\.[0-9]+\.[0-9]+)$ ]] || { echo "unrecognized \$id '$id' in $spec" >&2; exit 1; }
  echo "${BASH_REMATCH[1]}"
}

rank=0
names=(fix feature breaking)
raise() {
  (( $1 > rank )) && rank=$1
  echo "${names[$1]}: $2" >&2
}

body=""
if git -C "$root" cat-file -e "$rev:CHANGELOG.md" 2>/dev/null; then
  body="$(git -C "$root" show "$rev:CHANGELOG.md" | "$scripts/changelog-section.sh" "$section")" \
    || { echo "CHANGELOG.md at $rev has no '## [$section]' section" >&2; exit 1; }
fi
grep -qx '### Breaking' <<< "$body" && raise 2 "CHANGELOG.md [$section] has ### Breaking"
grep -qx '### Added' <<< "$body" && raise 1 "CHANGELOG.md [$section] has ### Added"

for schema in channel_config lineup_config playout; do
  old="$(schema_version "$last" "$schema")"
  new="$(schema_version "$rev" "$schema")"
  [[ "$old" == "$new" ]] && continue
  # schema versions are 0.B.C: B is breaking, C is compatible
  if [[ "${old%.*}" != "${new%.*}" ]]; then
    raise 2 "schema $schema $old -> $new"
  else
    raise 1 "schema $schema $old -> $new"
  fi
done

# 0.x puts features and fixes in the patch number, like cargo
if (( major == 0 )); then
  case "$rank" in
    2) version="0.$((minor + 1)).0" ;;
    *) version="0.$minor.$((patch + 1))" ;;
  esac
else
  case "$rank" in
    2) version="$((major + 1)).0.0" ;;
    1) version="$major.$((minor + 1)).0" ;;
    *) version="$major.$minor.$((patch + 1))" ;;
  esac
fi

echo "level=${names[$rank]}"
echo "version=$version"
