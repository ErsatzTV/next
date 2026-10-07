#!/usr/bin/env bash
# prints the body of one "## [<section>]" heading; exits 3 if the heading is missing
set -euo pipefail

section="${1:?usage: changelog-section.sh <section> [<file>]}"

# the last section ends at the link references
awk -v heading="## [$section]" '
  { sub(/^\xef\xbb\xbf/, ""); sub(/\r$/, "") }
  found && (/^## \[/ || /^\[[^]]+\]: /) { exit }
  found && (started || NF) { started = 1; print }
  index($0, heading) == 1 { found = 1 }
  END { if (!found) exit 3 }
' "${2:--}"
