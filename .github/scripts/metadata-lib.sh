# shellcheck shell=bash
# Helpers for .github/workflows/metadata.yml. Tested by metadata-lib.test.sh.
# Sourced by bash. Needs jq.
#
# The workflow signs `metadata/metadata.json` (the model's and the dictionary's entry) and publishes it,
# with its signature and `metadata/custom_dict.json`, on the generated `metadata` branch. What the app
# checks is `resonance_core::signed_metadata`; docs/decisions.md D-28 has the why.

# The revision a tag publishes: metadata-v<N>, N a positive whole number without a leading zero.
metadata_revision() {
  if [[ $1 =~ ^metadata-v([1-9][0-9]*)$ ]]; then
    printf '%s\n' "${BASH_REMATCH[1]}"
  else
    printf '%s는 메타데이터 태그(metadata-vN)가 아닙니다\n' "$1" >&2
    return 1
  fi
}

# Is revision <N> the one that may be published after <last> (the revision on the `metadata` branch; empty or 0
# when nothing is published)? Exactly the next one: the app refuses a lower revision, and a gap would mean a
# publication was skipped by mistake. Prints the reason and returns 1 when not.
metadata_revision_problem() {
  local n=$1 last=${2:-0}
  if [ "$n" -eq $((last + 1)) ]; then
    return 0
  fi
  printf '리비전 %s는 마지막으로 발행된 %s의 다음 번호(%s)여야 합니다\n' "$n" "$last" "$((last + 1))"
  return 1
}

# Does the source (stdin) look like the file the workflow can publish? Prints the first problem and returns 1.
# The workflow itself adds `revision` and `dictionary.sha256`, so the source must not have them.
metadata_source_problem() {
  local json
  json=$(cat)
  if ! jq -e 'type == "object"' >/dev/null 2>&1 <<<"$json"; then
    printf 'metadata.json이 JSON 객체가 아닙니다\n'
    return 1
  fi
  local checks=(
    '(.model.latest_version | type == "string" and length > 0)@@model.latest_version가 비어 있습니다'
    '(.model.download_url | type == "string" and startswith("https://"))@@model.download_url이 https 주소가 아닙니다'
    '(.model.sha256 | type == "string" and test("^[0-9a-f]{64}$"))@@model.sha256이 소문자 16진수 64자가 아닙니다'
    '(.dictionary.version | type == "string" and length > 0)@@dictionary.version이 비어 있습니다'
    '(has("revision") | not)@@revision은 태그가 정하므로 원본에 있으면 안 됩니다 (revision)'
    '(.dictionary | has("sha256") | not)@@dictionary.sha256은 워크플로가 계산하므로 원본에 있으면 안 됩니다 (dictionary.sha256)'
  )
  local check expr reason
  for check in "${checks[@]}"; do
    expr=${check%%@@*}
    reason=${check#*@@}
    if ! jq -e "$expr" >/dev/null 2>&1 <<<"$json"; then
      printf '%s\n' "$reason"
      return 1
    fi
  done
}

# The SHA-256 of a file, lower-case hex.
metadata_sha256() {
  sha256sum -- "$1" | cut -d' ' -f1
}

# The published metadata.json: the source (stdin) with the revision first and the dictionary's hash added.
metadata_publish_json() {
  local revision=$1 dictionary_sha256=$2
  if ! [[ $revision =~ ^[1-9][0-9]*$ ]]; then
    printf '리비전 %s는 양의 정수가 아닙니다\n' "$revision" >&2
    return 1
  fi
  jq --argjson revision "$revision" --arg sha "$dictionary_sha256" \
    '{revision: $revision} + . | .dictionary.sha256 = $sha'
}
