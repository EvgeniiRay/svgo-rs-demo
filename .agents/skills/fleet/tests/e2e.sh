#!/bin/bash
# e2e gate for the generic fleet kit: static pins that keep the kit's
# promises, then the offline smokes (watcher, tick-loop, timeout,
# durability, kill-handle, one-shot, winddown-topology, worker-topology).
# Offline, deterministic, no tmux, no network, no external service.
set -u
cd "$(dirname "${BASH_SOURCE[0]}")/../../../.."
KIT=.agents/skills/fleet
SKILL=$KIT/SKILL.md
WATCHER=$KIT/watcher.sh
fails=0
pass() { echo "ok   $1"; }
fail() { echo "FAIL $1"; fails=$((fails + 1)); }
pin() { grep -Eq "$2" "$1" 2>/dev/null && pass "$3" || fail "$3"; }
pin_absent() { grep -Eq "$2" "$1" 2>/dev/null && fail "$3" || pass "$3"; }

[ -f "$SKILL" ] && pass "exists: $SKILL" || fail "missing: $SKILL"
[ -f "$WATCHER" ] && pass "exists: $WATCHER" || fail "missing: $WATCHER"
[ "$fails" -eq 0 ] || { echo "fleet e2e: RED — kit artifacts missing ($fails)"; exit 1; }

# --- SKILL.md pins: the generic kit promises
pin "$SKILL" '^name: fleet$' "SKILL.md frontmatter name is fleet"
pin "$SKILL" 'muse --yolo -w create' "launcher table keeps the Muse Code row"
pin "$SKILL" 'same engine you are running as' "launcher table picks by the operator engine, no detection code"
pin "$SKILL" 'S=/tmp/fleet-<engagement>' "tick-loop snippet keeps its state-dir anchor (TEST-003 extractability)"
pin_absent "$SKILL" 'set-option[^|]*@fleet_' "SKILL.md never writes @fleet_* tmux options"
pin "$SKILL" 'normal agent review' "SKILL.md defines the reviewer workflow as normal agent review"
pin "$SKILL" 'reviewer never merges' "SKILL.md keeps merging out of the reviewer role"
pin "$SKILL" 're-reviews only the changed files' "SKILL.md bounds the review loop to changed files"
pin "$SKILL" 'counts toward the worker count' "SKILL.md counts reviewers toward the worker count"
[ -f "$KIT/reference/linux-host.md" ] && pass "Linux runbook present" \
  || fail "Linux runbook present"
[ -f "$KIT/reference/herdr-backend.md" ] && pass "Herdr backend mapping present" \
  || fail "Herdr backend mapping present"
pin "$SKILL" 'reference/herdr-backend\.md' "SKILL.md points at the Herdr backend mapping"
pin "$SKILL" 'reference/herdr-core-backend\.md' "SKILL.md points at the herdr-core backend mapping"
[ -f "$KIT/reference/herdr-core-backend.md" ] && pass "herdr-core backend mapping present" \
  || fail "herdr-core backend mapping present"
pin "$KIT/reference/herdr-core-backend.md" 'reviewer is an ordinary worker' "herdr-core backend mapping keeps reviewers as ordinary workers"
pin "$KIT/reference/herdr-core-backend.md" 'HC_SOCKET' "herdr-core backend mapping keeps the core-socket guard"
pin "$KIT/reference/herdr-core-backend.md" 'hc run --bg --no-parent' "herdr-core backend mapping starts detached fleet lanes"
pin "$KIT/reference/herdr-core-backend.md" 'reports no parent' "herdr-core backend mapping verifies detached lanes have no parent"
pin "$KIT/reference/herdr-core-backend.md" 'hc --skill' "herdr-core backend mapping defers syntax to the installed hc manual"
pin "$KIT/reference/herdr-core-backend.md" 'recreate-and-resume' "herdr-core backend mapping moves sessions by recreate-and-resume only"
pin "$KIT/reference/herdr-backend.md" 'normal agent|reviewer is an ordinary worker' "Herdr backend mapping keeps reviewers as ordinary workers"
[ -x "$KIT/herdr/fleet-herdr" ] && pass "fleet-herdr is vendored and executable" \
  || fail "fleet-herdr is vendored and executable"
pin "$KIT/herdr/fleet-herdr" 'want claude.codex.muse' "fleet-herdr accepts only the generic engine kinds"
pin "$KIT/herdr/fleet-herdr" 'FLEET_OS' "fleet-herdr detects its host OS"
pin "$KIT/herdr/fleet-herdr" 'fleet_jq' "fleet-herdr reads Herdr replies through fleet_jq"
pin "$KIT/herdr/fleet-herdr" 'MSYS_NO_PATHCONV' "fleet-herdr disables MSYS argument conversion on Windows"
pin "$KIT/reference/herdr-backend.md" 'Git Bash' "Herdr backend mapping covers Windows under Git Bash"

# --- watcher pins: the parse-free staleness contract, optional wake source
pin "$WATCHER" 'one parse-free check' "watcher declares the staleness contract"
pin "$WATCHER" 'FLEET_STALE_SECS' "watcher keeps the FLEET_STALE_SECS knob"
pin "$WATCHER" 'FLEET_TICK_SECS' "watcher keeps the FLEET_TICK_SECS knob"
pin "$WATCHER" 'FLEET_LIST_IDS_CMD' "watcher keeps the FLEET_LIST_IDS_CMD override"
pin "$WATCHER" 'NO default wake source' "watcher documents that no wake source is the default"
# wake-append must precede seen-append (crash-window ordering) — match
# the wake-path $id append, NOT line 15's startup seen-set creation (`: >>seen`)
wake_line=$(grep -n ' wake %s' "$WATCHER" | head -1 | cut -d: -f1)
seen_line=$(grep -n '"\$id" >>"\$STATE/seen"' "$WATCHER" | head -1 | cut -d: -f1)
if [ -n "$wake_line" ] && [ -n "$seen_line" ] && [ "$wake_line" -lt "$seen_line" ]; then
  pass "wake append precedes seen append"
else
  fail "wake append precedes seen append — got wake:$wake_line seen:$seen_line"
fi

[ "$fails" -eq 0 ] || { echo "fleet e2e: $fails static pin(s) failed"; exit 1; }

bash "$KIT/tests/watcher-smoke.sh" || { echo "fleet e2e: watcher-smoke FAILED"; exit 1; }
bash "$KIT/tests/watcher-timeout-smoke.sh" || { echo "fleet e2e: watcher-timeout-smoke FAILED"; exit 1; }
bash "$KIT/tests/tick-loop-smoke.sh" || { echo "fleet e2e: tick-loop-smoke FAILED"; exit 1; }
bash "$KIT/tests/operator-durability-smoke.sh" || { echo "fleet e2e: operator-durability-smoke FAILED"; exit 1; }
bash "$KIT/tests/kill-handle-smoke.sh" || { echo "fleet e2e: kill-handle-smoke FAILED"; exit 1; }
bash "$KIT/tests/one-shot-smoke.sh" || { echo "fleet e2e: one-shot-smoke FAILED"; exit 1; }
bash "$KIT/tests/winddown-topology-smoke.sh" || { echo "fleet e2e: winddown-topology-smoke FAILED"; exit 1; }
bash "$KIT/tests/worker-topology-smoke.sh" || { echo "fleet e2e: worker-topology-smoke FAILED"; exit 1; }
bash "$KIT/herdr/tests/herdr-smoke.sh" || { echo "fleet e2e: herdr-smoke FAILED"; exit 1; }
echo "fleet e2e: ok"
