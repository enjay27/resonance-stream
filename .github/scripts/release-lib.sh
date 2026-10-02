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
