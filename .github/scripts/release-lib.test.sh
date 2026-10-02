#!/usr/bin/env bash
# Tests for release-lib.sh. Run: bash .github/scripts/release-lib.test.sh
# shellcheck source-path=SCRIPTDIR
set -uo pipefail
cd "$(dirname "$0")" || exit 1
source ./release-lib.sh

fails=0
eq() { # eq <name> <expected> <actual>
  if [ "$2" == "$3" ]; then echo "ok   $1"; else echo "FAIL $1"; echo "  expected: $2"; echo "  actual:   $3"; fails=$((fails + 1)); fi
}

# --- release_version: a stable tag v<major>.<minor>.<patch> gives its version ---
eq "stable tag"            "0.6.1"  "$(release_version v0.6.1)"
eq "multi-digit"           "12.30.4" "$(release_version v12.30.4)"
eq "candidate tag refused" "failed" "$(release_version v0.6.0-rc.main-ui 2>/dev/null || echo failed)"
eq "no v refused"          "failed" "$(release_version 0.6.1 2>/dev/null || echo failed)"
eq "two parts refused"     "failed" "$(release_version v0.6 2>/dev/null || echo failed)"
eq "suffix refused"        "failed" "$(release_version v0.6.1.2 2>/dev/null || echo failed)"
eq "empty refused"         "failed" "$(release_version '' 2>/dev/null || echo failed)"
eq "refusal says why"      "v0.6.0-rc.x는 정식 릴리스 태그(vX.Y.Z)가 아닙니다" "$(release_version v0.6.0-rc.x 2>&1 >/dev/null || true)"

# --- release_asset: the exe's file name on the release ---
eq "asset name" "Resonance-Stream-v0.6.1.exe" "$(release_asset v0.6.1)"

# --- release_download_url: where the app will fetch the exe (and <url>.sig) ---
eq "download url" "https://github.com/o/r/releases/download/v0.6.1/Resonance-Stream-v0.6.1.exe" "$(release_download_url o/r v0.6.1)"

# --- release_manifest: latest.json, the app's update feed (notes on stdin) ---
json=$(printf 'Line one\n"quoted" \\ back\n' | release_manifest 0.6.1 2026-10-02T09:00:00Z https://x/y.exe 'c2lnbmF0dXJl')
eq "version"   "0.6.1"                 "$(jq -r .version <<<"$json")"
eq "pub_date"  "2026-10-02T09:00:00Z"  "$(jq -r .pub_date <<<"$json")"
eq "url"       "https://x/y.exe"       "$(jq -r .url <<<"$json")"
eq "signature" "c2lnbmF0dXJl"          "$(jq -r .signature <<<"$json")"
eq "notes survive quotes and newlines" $'Line one\n"quoted" \\ back' "$(jq -r .notes <<<"$json")"
eq "no notes -> empty string" "" "$(printf '' | release_manifest 0.6.1 2026-10-02T09:00:00Z https://x/y.exe sig | jq -r .notes)"
eq "only the five fields" "notes pub_date signature url version" "$(jq -r 'keys | join(" ")' <<<"$json")"
# The signature file is one line; a trailing newline must not end up in the feed.
eq "signature trimmed" "abc" "$(printf '' | release_manifest 0.6.1 2026-10-02T09:00:00Z https://x/y.exe $'abc\n' | jq -r .signature)"

echo
if [ "$fails" -eq 0 ]; then echo "all passed"; else echo "$fails failed"; exit 1; fi
