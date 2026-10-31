#!/usr/bin/env bash
# usage: ffmpeg-pin.sh [check | update <tag>]
# prints "<tag> <repo> <image>" for the ErsatzTV-ffmpeg build pinned in .github/ffmpeg.json
# check: also verifies the pin against GitHub and GHCR, and that docker/Dockerfile defaults to it
# update: pins <tag> (X.Y.Z-N or X.Y.Z-<sha8>-develop) in .github/ffmpeg.json and docker/Dockerfile, then checks
set -euo pipefail

root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
pin="$root/.github/ffmpeg.json"

locate() {
  if [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+-[0-9]+$ ]]; then
    repo=ErsatzTV/ErsatzTV-ffmpeg image=ghcr.io/ersatztv/ersatztv-ffmpeg
  elif [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+-[0-9a-f]{8}-develop$ ]]; then
    repo=ErsatzTV/ffmpeg-develop-builds image=ghcr.io/ersatztv/ersatztv-ffmpeg-develop
  else
    echo "unrecognized ffmpeg tag '$1'; expected X.Y.Z-N or X.Y.Z-<sha8>-develop" >&2
    exit 1
  fi
}

if [[ "${1:-}" == update ]]; then
  tag="${2:?usage: ffmpeg-pin.sh update <tag>}"
  locate "$tag"
  digest="$(docker buildx imagetools inspect "$image:$tag" --format '{{json .Manifest}}' | jq -er .digest)"
  # a release can't publish the hash of its own SHA256SUMS, so it is computed here
  sums="$(gh release download "$tag" --repo "$repo" --pattern SHA256SUMS --output - | sha256sum | cut -d' ' -f1)"
  jq -n --arg tag "$tag" --arg digest "$digest" --arg sums "$sums" \
    '{tag: $tag, digest: $digest, sha256sums: $sums}' > "$pin"
  dockerfile="$root/docker/Dockerfile"
  sed "s|^ARG FFMPEG_IMAGE=.*|ARG FFMPEG_IMAGE=$image:$tag@$digest|" "$dockerfile" > "$dockerfile.tmp"
  mv "$dockerfile.tmp" "$dockerfile"
  set -- check
fi

tag="$(jq -er .tag "$pin")"
digest="$(jq -er .digest "$pin")"
sums="$(jq -er .sha256sums "$pin")"
locate "$tag"
[[ "$digest" =~ ^sha256:[0-9a-f]{64}$ ]] || { echo "bad digest '$digest' in .github/ffmpeg.json" >&2; exit 1; }
[[ "$sums" =~ ^[0-9a-f]{64}$ ]] || { echo "bad sha256sums '$sums' in .github/ffmpeg.json" >&2; exit 1; }
ref="$image:$tag@$digest"

if [[ "${1:-}" == check ]]; then
  fail() { echo "ffmpeg pin $tag: $1" >&2; exit 1; }
  default="$(sed -n 's/^ARG FFMPEG_IMAGE=//p' "$root/docker/Dockerfile")"
  [[ "$default" == "$ref" ]] || fail "docker/Dockerfile ARG FFMPEG_IMAGE is '$default', expected '$ref'"
  others="$(grep -n 'ersatztv-ffmpeg' "$root/docker/Dockerfile" | grep -v '^[0-9]*:ARG FFMPEG_IMAGE=' || true)"
  [[ -z "$others" ]] || fail "docker/Dockerfile names ersatztv-ffmpeg outside ARG FFMPEG_IMAGE: $others"

  is_draft="$(gh release view "$tag" --repo "$repo" --json isDraft --jq .isDraft)" || fail "no release in $repo"
  [[ "$is_draft" == false ]] || fail "release in $repo is a draft"
  actual="$(gh release download "$tag" --repo "$repo" --pattern SHA256SUMS --output - | sha256sum | cut -d' ' -f1)"
  [[ "$actual" == "$sums" ]] || fail "SHA256SUMS hashes to $actual, pinned $sums"
  actual="$(docker buildx imagetools inspect "$image:$tag" --format '{{json .Manifest}}' | jq -er .digest)"
  [[ "$actual" == "$digest" ]] || fail "$image:$tag is $actual, pinned $digest"
fi

echo "$tag $repo $ref"
