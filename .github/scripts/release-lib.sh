# shellcheck shell=bash
# Helpers for .github/workflows/release.yml. Tested by release-lib.test.sh.
# Sourced by bash.

# The version of a stable release tag (v<major>.<minor>.<patch>); anything else
# -- a candidate tag (v0.6.0-rc.x), a typo -- is refused.
release_version() {
  if [[ $1 =~ ^v([0-9]+\.[0-9]+\.[0-9]+)$ ]]; then
    printf '%s\n' "${BASH_REMATCH[1]}"
  else
    printf '%s는 정식 릴리스 태그(vX.Y.Z)가 아닙니다\n' "$1" >&2
    return 1
  fi
}

# The exe's file name on the release. The app fetches its signature from the
# same URL plus ".sig", so the two assets are always <name> and <name>.sig.
release_asset() {
  printf 'Resonance-Stream-%s.exe\n' "$1"
}

# Where the app downloads the exe from: <repo> is "owner/name".
release_download_url() {
  printf 'https://github.com/%s/releases/download/%s/%s\n' "$1" "$2" "$(release_asset "$2")"
}

# latest.json, the app's update feed: version, notes (stdin), pub_date, the exe's
# url and the base64 signature (as `tauri signer sign` writes it). The five
# field names are the updater's "dynamic" manifest format, so the official
# plugin could read this file too.
release_manifest() {
  local version=$1 pub_date=$2 url=$3 signature=$4
  jq -n --arg version "$version" --arg pub_date "$pub_date" --arg url "$url" \
    --arg signature "${signature//[$'\r\n']/}" --rawfile notes /dev/stdin \
    '{version: $version, notes: $notes, pub_date: $pub_date, url: $url, signature: $signature}'
}

# Would the app update from release <tag>? The release's asset names (one per
# line) and the live latest.json (stdin) are checked the way the app reads
# them: the three files exist, the feed is for this version, points at this
# release's exe and carries a signature. Prints the reason and returns 1 when
# something is off; prints nothing and returns 0 when it is fine. <slug> is
# the repository, "owner/name".
release_feed_problem() {
  local tag=$1 assets=$2 slug=$3 feed version exe want got
  feed=$(cat)
  if ! version=$(release_version "$tag" 2>/dev/null); then
    printf '%s는 정식 릴리스 태그(vX.Y.Z)가 아닙니다\n' "$tag"
    return 1
  fi
  exe=$(release_asset "$tag")
  for want in latest.json "$exe" "$exe.sig"; do
    if ! grep -Fxq -- "$want" <<<"$assets"; then
      printf '릴리스 %s에 %s 파일이 없습니다\n' "$tag" "$want"
      return 1
    fi
  done
  if ! jq -e 'type == "object"' >/dev/null 2>&1 <<<"$feed"; then
    printf 'latest.json이 JSON 객체가 아닙니다 (앞부분: %s)\n' "$(head -c 80 <<<"$feed" | tr '\n' ' ')"
    return 1
  fi
  got=$(jq -r '(.version // "") | tostring | ltrimstr("v")' <<<"$feed")
  if [ "$got" != "$version" ]; then
    printf 'latest.json의 version이 %s입니다 (릴리스 %s는 %s)\n' "$got" "$tag" "$version"
    return 1
  fi
  got=$(jq -r '(.url // "") | tostring' <<<"$feed")
  want=$(release_download_url "$slug" "$tag")
  if [ "$got" != "$want" ]; then
    printf 'latest.json의 url이 이 릴리스의 exe가 아닙니다: %s (기대: %s)\n' "$got" "$want"
    return 1
  fi
  if [ -z "$(jq -r '(.signature // "") | tostring' <<<"$feed")" ]; then
    printf 'latest.json의 signature가 비어 있습니다\n'
    return 1
  fi
}
