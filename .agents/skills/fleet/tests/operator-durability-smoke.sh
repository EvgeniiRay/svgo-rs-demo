#!/bin/bash
# Guards the SKILL.md durable-supervision contract: the Tick step must not
# demand unenforceable infinite in-turn presence, and must instead require an
# upfront stop condition plus an armed recurring schedule that survives the
# end of any operator turn. Asserts stable anchor phrases so a future edit
# dropping any clause goes red; kill-verify by deleting each anchor in turn
# (every deletion must fail).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../../../.."
SKILL=.agents/skills/fleet/SKILL.md
fail() { echo "FAIL: $1" >&2; exit 1; }

[ -f "$SKILL" ] || fail "skill playbook not found at $SKILL"

# 1. The absolute never-end-the-turn ban is gone (it died with the first
#    bounded operator turn).
grep -q 'NEVER end your turn while' "$SKILL" \
  && fail "absolute in-turn ban still present (unenforceable)"

# 2. The replacement invariant: never unattended AND unscheduled.
grep -q 'neither an in-turn reader nor an armed schedule' "$SKILL" \
  || fail "missing unattended/unscheduled invariant"

# 3. Every engagement declares its stop condition up front.
grep -q 'stop/wind-down condition' "$SKILL" \
  || fail "missing stop/wind-down condition requirement"
grep -q 'up front' "$SKILL" \
  || fail "stop condition not required up front"

# 4. The schedule is armed at dispatch and re-armed by each tick.
grep -q 'armed schedule' "$SKILL" \
  || fail "missing armed-schedule requirement"
grep -q 're-arm' "$SKILL" \
  || fail "missing schedule re-arm semantics"
grep -q 'self-check at ~10 min' "$SKILL" \
  || fail "scheduled self-check has no cadence"

# 4b. In-turn presence is bounded and the schedule is cancelled at wind-down.
grep -q 'Bound in-turn presence' "$SKILL" \
  || fail "missing bounded in-turn presence"
grep -q 'cancel the tick schedule' "$SKILL" \
  || fail "wind-down does not cancel the tick schedule"

# 5. Durable handles a fresh reader can find (dispatch tasks have no
#    engagement record beyond the state dir).
grep -q 'stop-condition' "$SKILL" \
  || fail "missing state-dir stop-condition handle"
grep -q 'tick-schedule' "$SKILL" \
  || fail "missing state-dir tick-schedule handle"

# 6. The resume contract names the adoption trigger (not just the steps).
grep -q 'summons the next reader' "$SKILL" \
  || fail "resume contract names no adoption trigger"

# 6b. A fresh session arms its own schedule (no one-tick-and-abandon resume).
#    Every anchor greps the whitespace-normalized playbook, so prose
#    reflows never red this smoke for a non-behavioral reason.
NORM=$(tr -s '[:space:]' ' ' < "$SKILL")
anchor() { # $1 = literal phrase, $2 = failure message
  grep -qF -- "$1" <<<"$NORM" || fail "$2"
}
anchor 'arm a new schedule' \
  "fresh operator session does not arm its own schedule"
anchor 'cancel command before arming your own' \
  "resume does not cancel the inherited schedule before re-arming"
anchor 'never the target' \
  "host one-shot path may target the bare operator shell"
anchor 'Arm a host one-shot only when' \
  "host one-shot arm has no explicit in-pane precondition"
anchor 'do not dispatch, do not arm' \
  "socket/pane gate is not pre-dispatch"
# shellcheck disable=SC2016 # anchor phrases are intentionally literal (grep -F)
anchor 'SOCK="${TMUX:-}"' \
  "socket resolve is not unset-safe"
anchor 'never execute it' \
  "resume may execute tick-schedule as a script"
anchor 'also copy line 1' \
  "goal handle copy missing"

# 6c. Round-4 safety clauses each have a keyed anchor (kill-verify each
#    deletion in turn; every deletion must fail).
anchor 'types only when both match the arm-time values' \
  "host one-shot has no pane pid/command guard"
anchor 'A missed delivery ends the chain' \
  "missed-delivery chain end missing"
anchor 'delivery receipt is missing' \
  "invariant exception for a missed receipt missing"

echo "operator-durability-smoke: ok"
