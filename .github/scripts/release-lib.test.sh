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

# --- release_feed_problem: would the app update from this release? (feed on stdin) ---
assets=$'latest.json\nResonance-Stream-v0.6.1.exe\nResonance-Stream-v0.6.1.exe.sig\nSHA256SUMS.txt'
good_feed() { printf '' | release_manifest 0.6.1 2026-10-02T09:00:00Z "$(release_download_url o/r v0.6.1)" c2ln; }
problem() { # problem <tag> <assets> <feed>  -> "rc|reason"
  local out rc
  out=$(release_feed_problem "$1" "$2" o/r <<<"$3" 2>&1); rc=$?
  printf '%s|%s' "$rc" "$out"
}
has() { # has <name> <text> <needle>
  if [[ $2 == *"$3"* ]]; then echo "ok   $1"; else echo "FAIL $1"; echo "  wanted '$3' in: $2"; fails=$((fails + 1)); fi
}

eq "a healthy release has no problem" "0|" "$(problem v0.6.1 "$assets" "$(good_feed)")"

res=$(problem v0.6.1 "${assets/latest.json/}" "$(good_feed)")
eq  "missing latest.json: refused" "1" "${res%%|*}"
has "missing latest.json: named"   "$res" "latest.json"
res=$(problem v0.6.1 "${assets/Resonance-Stream-v0.6.1.exe.sig/}" "$(good_feed)")
eq  "missing .sig: refused" "1" "${res%%|*}"
has "missing .sig: named"   "$res" "Resonance-Stream-v0.6.1.exe.sig"
res=$(problem v0.6.1 $'latest.json\nResonance-Stream-v0.6.1.exe.sig' "$(good_feed)")
eq  "missing exe: refused" "1" "${res%%|*}"
has "missing exe: named"   "$res" "Resonance-Stream-v0.6.1.exe"

old_feed=$(printf '' | release_manifest 0.6.0 2026-10-02T09:00:00Z "$(release_download_url o/r v0.6.1)" c2ln)
res=$(problem v0.6.1 "$assets" "$old_feed")
eq  "feed for another version: refused" "1" "${res%%|*}"
has "feed for another version: both versions named" "$res" "0.6.0"

wrong_url=$(printf '' | release_manifest 0.6.1 2026-10-02T09:00:00Z https://example.com/app.exe c2ln)
res=$(problem v0.6.1 "$assets" "$wrong_url")
eq  "feed url elsewhere: refused" "1" "${res%%|*}"
has "feed url elsewhere: named"   "$res" "url"

no_sig=$(printf '' | release_manifest 0.6.1 2026-10-02T09:00:00Z "$(release_download_url o/r v0.6.1)" '')
res=$(problem v0.6.1 "$assets" "$no_sig")
eq  "empty signature: refused" "1" "${res%%|*}"
has "empty signature: named"   "$res" "signature"

res=$(problem v0.6.1 "$assets" "<html>Not Found</html>")
eq  "feed that is not JSON: refused" "1" "${res%%|*}"
has "feed that is not JSON: named"   "$res" "JSON"
eq  "empty feed: refused" "1" "$(problem v0.6.1 "$assets" "" | cut -d'|' -f1)"
eq  "a candidate tag is never a stable release" "1" "$(problem v0.6.1-rc.x "$assets" "$(good_feed)" | cut -d'|' -f1)"

# --- check-release-feed.sh: the live check, with gh and curl faked ---
fake=$(mktemp -d)
cat > "$fake/gh" <<'FAKE'
#!/usr/bin/env bash
[ -n "${FAKE_GH_FAIL:-}" ] && { echo "HTTP 404" >&2; exit 1; }
printf '%s\n' "$FAKE_RELEASE_JSON"
FAKE
cat > "$fake/curl" <<'FAKE'
#!/usr/bin/env bash
# FAKE_CURL_FAILS=n: the first n calls fail (a feed that is not up yet).
n=$(cat "$FAKE_COUNT" 2>/dev/null || echo 0)
echo $((n + 1)) > "$FAKE_COUNT"
if [ -n "${FAKE_CURL_FAIL:-}" ] || [ "$n" -lt "${FAKE_CURL_FAILS:-0}" ]; then exit 22; fi
printf '%s\n' "$FAKE_FEED"
FAKE
chmod +x "$fake/gh" "$fake/curl"
release_json() { # release_json <tag> -> what GET releases/latest returns
  jq -n --arg tag "$1" --arg exe "$(release_asset "$1")" \
    '{tag_name: $tag, assets: [{name: "latest.json"}, {name: $exe}, {name: ($exe + ".sig")}]}'
}
live() { # live <repo> [tag] [ENV=value ...] -> "rc|output" of check-release-feed.sh
  local out rc
  rm -f "$fake/count"
  out=$(env PATH="$fake:$PATH" FAKE_COUNT="$fake/count" FEED_ATTEMPTS=3 FEED_PAUSE=0 \
    FAKE_RELEASE_JSON="$(release_json v0.6.1)" FAKE_FEED="$(good_feed)" "${@:3}" \
    bash ./check-release-feed.sh "$1" ${2:+"$2"} 2>&1); rc=$?
  printf '%s|%s' "$rc" "$out"
}

res=$(live o/r)
eq "live: a healthy latest release passes" "0" "${res%%|*}"
eq "live: ... with the expected tag" "0" "$(live o/r v0.6.1 | cut -d'|' -f1)"
res=$(live o/r v0.7.0)
eq  "live: latest is not the release just published: fails" "1" "${res%%|*}"
has "live: ... naming both tags" "$res" "v0.6.1"
res=$(live o/r "" FAKE_GH_FAIL=1)
eq  "live: no stable release at all: fails" "1" "${res%%|*}"
has "live: ... is an error annotation" "$res" "::error title="
has "live: ... says why" "$res" "정식 릴리스(latest)가 없습니다"
res=$(live o/r "" FAKE_CURL_FAIL=1)
eq  "live: feed that cannot be fetched (the app's 404): fails" "1" "${res%%|*}"
has "live: ... names the feed" "$res" "latest.json"
eq  "live: a feed that is not up yet is waited for" "0" "$(live o/r v0.6.1 FAKE_CURL_FAILS=2 | cut -d'|' -f1)"
eq  "live: ... but not forever" "1" "$(live o/r v0.6.1 FAKE_CURL_FAILS=3 | cut -d'|' -f1)"
rm -rf "$fake"

echo
if [ "$fails" -eq 0 ]; then echo "all passed"; else echo "$fails failed"; exit 1; fi
