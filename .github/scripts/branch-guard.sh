# Helper for .github/workflows/test-branch-guard.yml. Tested by branch-guard.test.sh.

# Succeeds, and says why on stdout, when a pull request from this head branch
# must not be merged into main: `test/*` branches hold throw-away test tooling
# (notebooks, fixtures) that is run on a person's PC and never shipped.
never_merges_into_main() {
  local head
  head=$(printf '%s' "${1:-}" | tr '[:upper:]' '[:lower:]')
  case "$head" in
    test/*)
      echo "test/* branches are never merged into main"
      return 0
      ;;
  esac
  return 1
}
