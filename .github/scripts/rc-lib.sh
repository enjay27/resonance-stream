# Helpers for .github/workflows/release-candidate.yml. Tested by rc-lib.test.sh.
# Sourced by bash; every function reads its list input from stdin.

# The feature part of a candidate tag: the merged branch's name without its
# first prefix (candidate/, claude/, feat/ ...), lower case, only [a-z0-9-].
rc_feature() {
  local name=${1#*/}
  name=$(printf '%s' "$name" | tr '[:upper:]' '[:lower:]' | sed -E 's/[^a-z0-9-]+/-/g; s/-+/-/g; s/^-//; s/-$//')
  printf '%s\n' "${name:-build}"
}

# The next free tag v<version>-rc.<feature>[.<n>], given the existing tags.
rc_tag() {
  local version=$1 feature=$2 base tags n
  base="v${version}-rc.${feature}"
  tags=$(cat)
  if ! grep -qxF "$base" <<<"$tags"; then
    printf '%s\n' "$base"
    return
  fi
  n=2
  while grep -qxF "$base.$n" <<<"$tags"; do n=$((n + 1)); done
  printf '%s\n' "$base.$n"
}

# Candidate tags to delete so only the newest <keep> remain.
# Input: "<tag><TAB><created_at ISO-8601>" lines; non-candidate tags are ignored.
rc_prune() {
  local keep=$1
  { grep -E $'^v[^\t]*-rc\\.[^\t]*\t' || true; } | sort -t $'\t' -k2,2r | tail -n +"$((keep + 1))" | cut -f1
}

# "NOT VERIFIED: ..." notes of commit messages as a list, each once. A note
# wrapped over several lines (commit bodies wrap at ~72 columns) is joined; it
# ends at a blank line, the next note or a trailer ("Co-Authored-By: ...").
rc_unverified() {
  awk '
    function flush() { if (note != "" && !seen[note]++) print "- " note; note = ""; on = 0 }
    /^[[:space:]]*NOT VERIFIED:?/ {
      flush(); line = $0
      sub(/^[[:space:]]*NOT VERIFIED:?[[:space:]]*/, "", line)
      note = line; on = 1; next
    }
    on && (/^[[:space:]]*$/ || /^[A-Za-z-]+: /) { flush(); next }
    on { line = $0; gsub(/^[[:space:]]+|[[:space:]]+$/, "", line); note = note " " line }
    END { flush() }
  '
}

# The app version: [workspace.package] version of the root Cargo.toml (stdin).
rc_version() {
  sed -n '/^\[workspace\.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' | head -1
}

# The "changes vs main" list (stdin: one "- subject (`sha`)" line per commit);
# a build with nothing beyond main says so instead of leaving the section empty.
rc_changes() {
  local lines
  lines=$(grep -v '^[[:space:]]*$' || true)
  if [ -n "$lines" ]; then
    printf '%s\n' "$lines"
  else
    printf '%s\n' '- (없음 -- main과 같은 빌드)'
  fi
}
