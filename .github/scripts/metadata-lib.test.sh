#!/usr/bin/env bash
# Tests for metadata-lib.sh. Run: bash .github/scripts/metadata-lib.test.sh
# shellcheck source-path=SCRIPTDIR
set -uo pipefail
cd "$(dirname "$0")" || exit 1
source ./metadata-lib.sh

fails=0
eq() { # eq <name> <expected> <actual>
  if [ "$2" == "$3" ]; then echo "ok   $1"; else echo "FAIL $1"; echo "  expected: $2"; echo "  actual:   $3"; fails=$((fails + 1)); fi
}
has() { # has <name> <text> <needle>
  if [[ $2 == *"$3"* ]]; then echo "ok   $1"; else echo "FAIL $1"; echo "  wanted '$3' in: $2"; fails=$((fails + 1)); fi
}
run() { # run <cmd...> -> "rc|stdout+stderr"   (stdin is passed through)
  local out rc
  out=$("$@" 2>&1); rc=$?
  printf '%s|%s' "$rc" "$out"
}

# --- metadata_revision: the tag metadata-v<N> (N a positive whole number, no leading zero) ---
eq "tag 1"                 "1"      "$(metadata_revision metadata-v1)"
eq "tag 12"                "12"     "$(metadata_revision metadata-v12)"
eq "tag 100"               "100"    "$(metadata_revision metadata-v100)"
eq "zero refused"          "failed" "$(metadata_revision metadata-v0 2>/dev/null || echo failed)"
eq "leading zero refused"  "failed" "$(metadata_revision metadata-v07 2>/dev/null || echo failed)"
eq "dotted refused"        "failed" "$(metadata_revision metadata-v1.2 2>/dev/null || echo failed)"
eq "no number refused"     "failed" "$(metadata_revision metadata-v 2>/dev/null || echo failed)"
eq "release tag refused"   "failed" "$(metadata_revision v0.6.1 2>/dev/null || echo failed)"
eq "suffix refused"        "failed" "$(metadata_revision metadata-v3-rc 2>/dev/null || echo failed)"
eq "empty refused"         "failed" "$(metadata_revision '' 2>/dev/null || echo failed)"
eq "refusal says why"      "v0.6.1는 메타데이터 태그(metadata-vN)가 아닙니다" "$(metadata_revision v0.6.1 2>&1 >/dev/null || true)"

# --- metadata_revision_problem <N> <last published>: N is exactly one higher ---
eq  "the first publication is 1"      "0|" "$(run metadata_revision_problem 1 0)"
eq  "the next one is last + 1"        "0|" "$(run metadata_revision_problem 8 7)"
res=$(run metadata_revision_problem 9 7)
eq  "a gap is refused"                "1" "${res%%|*}"
has "a gap names both numbers"        "$res" "9"
has "a gap names both numbers (last)" "$res" "7"
res=$(run metadata_revision_problem 7 7)
eq  "the same revision again is refused" "1" "${res%%|*}"
res=$(run metadata_revision_problem 5 7)
eq  "an older one is refused (a tag cannot roll the app back)" "1" "${res%%|*}"
res=$(run metadata_revision_problem 1 0 )
eq  "no published file yet counts as 0" "0|" "$res"
eq  "empty last counts as 0"           "0|" "$(run metadata_revision_problem 1 '')"

# --- metadata_source_problem: what metadata/metadata.json must look like (stdin) ---
HASH=7dc18e6bd8d3d7404f63e8996bb6d44bf35256a20406c1105e37c2424b96847d
good_source() { jq -n --arg h "$HASH" '{model: {latest_version: "1.1.0", download_url: "https://huggingface.co/x/y.gguf", release_notes: "n", sha256: $h}, dictionary: {version: "1.0.6", updated_at: "2026-03-08"}}'; }
check() { run metadata_source_problem <<<"$1"; }
eq  "a good source has no problem" "0|" "$(check "$(good_source)")"
res=$(check 'not json')
eq  "not JSON refused" "1" "${res%%|*}"
res=$(check "$(good_source | jq 'del(.model.latest_version)')")
eq  "no model version refused" "1" "${res%%|*}"
has "no model version named"   "$res" "model.latest_version"
res=$(check "$(good_source | jq '.model.download_url = "http://example.com/m.gguf"')")
eq  "a model url that is not https refused" "1" "${res%%|*}"
has "the url is named"         "$res" "model.download_url"
res=$(check "$(good_source | jq '.model.sha256 = "abc"')")
eq  "a short model hash refused" "1" "${res%%|*}"
has "the hash is named"        "$res" "model.sha256"
res=$(check "$(good_source | jq '.model.sha256 = "7DC18E6BD8D3D7404F63E8996BB6D44BF35256A20406C1105E37C2424B96847D"')")
eq  "an upper-case model hash refused (one spelling)" "1" "${res%%|*}"
res=$(check "$(good_source | jq 'del(.dictionary.version)')")
eq  "no dictionary version refused" "1" "${res%%|*}"
has "the dictionary version is named" "$res" "dictionary.version"
res=$(check "$(good_source | jq '.revision = 3')")
eq  "a revision in the source refused (the tag decides it)" "1" "${res%%|*}"
has "the revision is named"    "$res" "revision"
res=$(check "$(good_source | jq '.dictionary.sha256 = "aa"')")
eq  "a dictionary hash in the source refused (the workflow computes it)" "1" "${res%%|*}"
has "the dictionary hash is named" "$res" "dictionary.sha256"

# The source in the repo is one the workflow can publish.
eq "the repo's own metadata/metadata.json has no problem" "0|" "$(run metadata_source_problem < ../../metadata/metadata.json)"

# --- metadata_sha256 <file> ---
tmp=$(mktemp -d)
printf 'abc' > "$tmp/f"
eq "sha256 of a file" "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" "$(metadata_sha256 "$tmp/f")"

# --- metadata_publish_json <revision> <dictionary sha256>: the published file (source on stdin) ---
pub=$(good_source | metadata_publish_json 12 "$(metadata_sha256 "$tmp/f")")
eq "revision is a number"       "12" "$(jq -r .revision <<<"$pub")"
eq "revision comes first"       "revision model dictionary" "$(jq -r 'keys_unsorted | join(" ")' <<<"$pub")"
eq "dictionary hash added"      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" "$(jq -r .dictionary.sha256 <<<"$pub")"
eq "the dictionary version stays" "1.0.6" "$(jq -r .dictionary.version <<<"$pub")"
eq "the model stays as written" "$(good_source | jq -S .model)" "$(jq -S .model <<<"$pub")"
kor=$(good_source | jq '.model.release_notes = "기본 번역 모델"' | metadata_publish_json 1 aa)
has "korean is written as it is" "$kor" "기본 번역 모델"
eq "a revision that is not a number refused" "1" "$(good_source | metadata_publish_json twelve aa >/dev/null 2>&1; echo $?)"

rm -rf "$tmp"
if [ "$fails" -ne 0 ]; then echo "$fails failed"; exit 1; fi
echo "all passed"
