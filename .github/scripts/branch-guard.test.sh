#!/usr/bin/env bash
# Tests for branch-guard.sh. Run: bash .github/scripts/branch-guard.test.sh
# shellcheck source-path=SCRIPTDIR
set -uo pipefail
cd "$(dirname "$0")" || exit 1
source ./branch-guard.sh

fails=0
eq() { # eq <name> <expected> <actual>
  if [ "$2" == "$3" ]; then echo "ok   $1"; else echo "FAIL $1"; echo "  expected: $2"; echo "  actual:   $3"; fails=$((fails + 1)); fi
}
refused() { if never_merges_into_main "$1" >/dev/null; then echo yes; else echo no; fi; }

# --- never_merges_into_main: test/* branches are throw-away tooling ---
eq "test/ is refused"            yes "$(refused test/w1-updater)"
eq "nested test/ is refused"     yes "$(refused test/w1/firewall)"
eq "case does not matter"        yes "$(refused Test/w1-feed)"
eq "claude/ is allowed"          no  "$(refused claude/popup-escape-closes)"
eq "candidate/ is allowed"       no  "$(refused candidate/favorites-stable-ids)"
eq "a name that merely contains test is allowed" no "$(refused claude/test-branch-guard)"
eq "tests/ is not test/"         no  "$(refused tests/x)"
eq "bare test is not a prefix"   no  "$(refused test)"
eq "empty is allowed"            no  "$(refused '')"
eq "the reason names the rule"   "test/* branches are never merged into main" "$(never_merges_into_main test/x)"

[ "$fails" -eq 0 ] && echo "all passed" || { echo "$fails failed"; exit 1; }
