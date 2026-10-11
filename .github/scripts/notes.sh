#!/usr/bin/env bash
set -euo pipefail

tag="${1:?usage: notes.sh <tag> [<last release>]}"
repo="${GITHUB_REPOSITORY:-ErsatzTV/next}"
root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
changelog="${CHANGELOG:-$root/CHANGELOG.md}"

section() { "$root/.github/scripts/changelog-section.sh" "$1" "$changelog"; }

is_blank() { [[ -z "${1//[[:space:]]/}" ]]; }

# generated so integrators can check compatibility without anyone maintaining it by hand
compatibility() {
  local name schema id version
  echo "### Compatibility"
  for name in "Channel config:channel_config" "Lineup config:lineup_config" "Playout:playout"; do
    schema="${name#*:}"
    id="$(jq -r '."$id"' "$root/schema/$schema.json")"
    [[ "$id" =~ /version/(([0-9]+\.[0-9]+)\.[0-9]+)$ ]] || { echo "unrecognized \$id '$id' in schema/$schema.json" >&2; exit 1; }
    version="${BASH_REMATCH[1]}"
    echo "- ${name%%:*}: reads versions \`${BASH_REMATCH[2]}.0\` to \`$version\`"
  done
  ffmpeg="$(grep -o 'ersatztv-ffmpeg:[^@[:space:]]*' "$root/docker/Dockerfile" | sort -u)"
  [[ "$(wc -l <<< "$ffmpeg")" == 1 && -n "$ffmpeg" ]] || { echo "expected one ersatztv-ffmpeg tag in docker/Dockerfile, found: $ffmpeg" >&2; exit 1; }
  ffmpeg="${ffmpeg#ersatztv-ffmpeg:}"
  echo "- ErsatzTV-ffmpeg: [\`$ffmpeg\`](https://github.com/ErsatzTV/ErsatzTV-ffmpeg/releases/tag/$ffmpeg), bundled in the docker images and recommended for the native binaries"
}

if [[ "$tag" =~ ^v([0-9]+\.[0-9]+\.[0-9]+)$ ]]; then
  version="${BASH_REMATCH[1]}"
  body="$(section "$version")" || { echo "CHANGELOG.md has no '## [$version]' section" >&2; exit 1; }
  is_blank "$body" && { echo "CHANGELOG.md section '## [$version]' is empty" >&2; exit 1; }
  compat="$(compatibility)"
  printf '## Release Notes\n%s\n\n%s\n' "$body" "$compat"
elif [[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+-([0-9a-f]{8})-develop$ ]]; then
  sha="${BASH_REMATCH[1]}"
  # vX.Y.Z is the next release, not a tag yet
  base="${2:?develop tags need the last release tag as the second argument}"
  body="$(section Unreleased)" || { echo "CHANGELOG.md has no '## [Unreleased]' section" >&2; exit 1; }
  # normal right after a release
  is_blank "$body" && body="No changelog entries since $base."
  compat="$(compatibility)"
  printf '## Release Notes\n%s\n\n%s\n\nFull changes: https://github.com/%s/compare/%s...%s\n' \
    "$body" "$compat" "$repo" "$base" "$sha"
else
  echo "unrecognized tag '$tag'; expected vX.Y.Z or vX.Y.Z-<sha8>-develop" >&2
  exit 1
fi
