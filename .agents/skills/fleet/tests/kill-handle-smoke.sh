#!/bin/bash
# Guards the SKILL.md watcher kill handle by EXTRACTING the canonical
# validated-kill block from the doc and executing it: a stale pid pointing at
# an unrelated process must NOT be killed, while the real watcher pid IS.
# No tmux, no external service. Deadline-polled, like the other fleet smokes.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../../../.."
SKILL=.agents/skills/fleet/SKILL.md
WATCHER=.agents/skills/fleet/watcher.sh
fail() { echo "FAIL: $1" >&2; exit 1; }

# 1. no bare kill-from-pid-file may remain: every SKILL.md kill must validate
#    ownership first (wind-down and the resume contract both killed raw pids).
# shellcheck disable=SC2016  # literal $(cat is the point: the bare-kill pattern must be gone
grep -q 'kill "$(cat' "$SKILL" && fail "SKILL.md still kills a raw pid-file pid without an ownership check"

# 2. exactly one canonical validated-kill block, so the procedure cannot drift
#    between copies.
RAW=$(python3 - "$SKILL" <<'PY'
import re, sys
text = open(sys.argv[1]).read()
blocks = re.findall(r'```sh\n(.*?)```', text, re.S)
picks = [b for b in blocks if '/pid' in b and 'kill' in b]
assert len(picks) == 1, f"expected exactly one validated-kill block, got {len(picks)}"
print(picks[0])
PY
)
[ -n "$RAW" ] || fail "validated-kill snippet not found in SKILL.md"
WDREF=$(awk '/^[0-9]+\. \*\*Wind-down\.\*\*/,/^## /' "$SKILL")
grep -q 'validated kill' <<<"$WDREF" || fail "wind-down must reference the canonical validated kill"
RESUMEREF=$(awk '/^## Resume contract/,0' "$SKILL")
grep -q 'validated kill' <<<"$RESUMEREF" || fail "resume contract must reference the canonical validated kill"
SNIPPET=$(grep -v '<engagement>' <<<"$RAW" | sed 's/^   //')
# shellcheck disable=SC2016  # literal $S is the point: the snippet must reference it
grep -q '"$S/pid"' <<<"$SNIPPET" || fail "snippet not anchored to the state dir"

D=$(mktemp -d)
DECOY=""; WPID=""; APID=""; BPID=""; EPID1=""; EPID2=""; NPID=""
trap '[ -n "$DECOY" ] && kill "$DECOY" 2>/dev/null || true; [ -n "$WPID" ] && kill "$WPID" 2>/dev/null || true; [ -n "$APID" ] && kill "$APID" 2>/dev/null || true; [ -n "$BPID" ] && kill "$BPID" 2>/dev/null || true; [ -n "$EPID1" ] && kill "$EPID1" 2>/dev/null || true; [ -n "$EPID2" ] && kill "$EPID2" 2>/dev/null || true; [ -n "$NPID" ] && kill "$NPID" 2>/dev/null || true; rm -rf "$D"' EXIT
mkdir -p "$D/state"
printf '%s\n' "$SNIPPET" >"$D/snippet.sh"

run_snippet() { # run_snippet <state-dir> — S=state dir, captures rc + stderr
  S="$1" bash -eu "$D/snippet.sh" 2>"$D/err"; echo "$?"
}

# 3. victim test: pid file points at an unrelated live process -> it survives
#    and the guard says so.
sleep 300 &
DECOY=$!
printf '%s\n' "$DECOY" >"$D/state/pid"
rc=$(run_snippet "$D/state")
[ "$rc" -eq 0 ] || fail "snippet exited rc=$rc on a stale pid ($(head -1 "$D/err" 2>/dev/null))"
kill -0 "$DECOY" 2>/dev/null || fail "validated kill murdered an unrelated pid $DECOY"
grep -qi 'not .*watcher\|stale' "$D/err" || fail "guard fired silently on a stale pid (no warning on stderr)"
grep -q '(ps: ' "$D/err" || fail "refusal hides what ps saw (cannot tell dead pid from odd launch form)"

# 4. positive control: the same snippet DOES kill a real watcher pid.
#    Launched by absolute path under COLUMNS=40: a narrow `ps` must still see
#    the state-dir token (the snippet asks for unlimited width).
mkdir -p "$D/wstate"; : >"$D/wstate/wake.txt"; touch "$D/t.jsonl"
FLEET_TICK_SECS=1 sh "$(pwd)/$WATCHER" "$D/wstate" "$D/t.jsonl" &
WPID=$!
deadline=$((SECONDS + 30))
while [ ! -s "$D/wstate/heartbeat" ]; do
  [ "$SECONDS" -lt "$deadline" ] || fail "timeout waiting for watcher start"
  sleep 0.2
done
[ "$(cat "$D/wstate/pid")" = "$WPID" ] || fail "watcher pid file does not match launched pid"
rc=$(COLUMNS=40 run_snippet "$D/wstate")
[ "$rc" -eq 0 ] || fail "snippet exited rc=$rc on a live watcher pid"
deadline=$((SECONDS + 30))
while kill -0 "$WPID" 2>/dev/null; do
  [ "$SECONDS" -lt "$deadline" ] || fail "validated kill did not stop the real watcher $WPID"
  sleep 0.2
done

# 5. fratricide arm: two sibling watchers; A's pid file points at B's pid ->
#    the engagement-A kill must refuse (B lives).
mkdir -p "$D/astate" "$D/bstate"; : >"$D/astate/wake.txt"; : >"$D/bstate/wake.txt"
FLEET_TICK_SECS=1 sh "$WATCHER" "$D/astate" "$D/t.jsonl" &
APID=$!
FLEET_TICK_SECS=1 sh "$WATCHER" "$D/bstate" "$D/t.jsonl" &
BPID=$!
deadline=$((SECONDS + 30))
while [ ! -s "$D/astate/heartbeat" ] || [ ! -s "$D/bstate/heartbeat" ]; do
  [ "$SECONDS" -lt "$deadline" ] || fail "timeout waiting for sibling watchers"
  sleep 0.2
done
printf '%s\n' "$BPID" >"$D/astate/pid"
rc=$(run_snippet "$D/astate")
[ "$rc" -eq 0 ] || fail "snippet exited rc=$rc on a sibling pid ($(head -1 "$D/err" 2>/dev/null))"
kill -0 "$BPID" 2>/dev/null || fail "engagement-A kill murdered sibling watcher $BPID"
kill -0 "$APID" 2>/dev/null || fail "sibling setup died unexpectedly"
grep -q "not this engagement's watcher" "$D/err" || fail "fratricide refusal said nothing about the engagement"

# 6. prefix arm: sibling state dirs where one name prefixes the other (demo
#    vs demo2) — a bare-substring match kills the sibling; token-boundary
#    matching must refuse.
mkdir -p "$D/demo" "$D/demo2"; : >"$D/demo/wake.txt"; : >"$D/demo2/wake.txt"
FLEET_TICK_SECS=1 sh "$WATCHER" "$D/demo" "$D/t.jsonl" &
EPID1=$!
FLEET_TICK_SECS=1 sh "$WATCHER" "$D/demo2" "$D/t.jsonl" &
EPID2=$!
deadline=$((SECONDS + 30))
while [ ! -s "$D/demo/heartbeat" ] || [ ! -s "$D/demo2/heartbeat" ]; do
  [ "$SECONDS" -lt "$deadline" ] || fail "timeout waiting for prefix sibling watchers"
  sleep 0.2
done
printf '%s\n' "$EPID2" >"$D/demo/pid"
rc=$(run_snippet "$D/demo")
[ "$rc" -eq 0 ] || fail "snippet exited rc=$rc on a prefix-sibling pid ($(head -1 "$D/err" 2>/dev/null))"
kill -0 "$EPID2" 2>/dev/null || fail "demo kill murdered prefix sibling watcher $EPID2"

# 7. transcript-less arm: a watcher started with no transcript args ends its
#    cmdline at the state dir — the kill must still match it
#    (trailing-space-only matching calls it stale and the operator stacks a
#    second watcher on top).
mkdir -p "$D/ntstate"; : >"$D/ntstate/wake.txt"
FLEET_TICK_SECS=1 sh "$WATCHER" "$D/ntstate" &
NPID=$!
deadline=$((SECONDS + 30))
while [ ! -s "$D/ntstate/heartbeat" ]; do
  [ "$SECONDS" -lt "$deadline" ] || fail "timeout waiting for transcript-less watcher"
  sleep 0.2
done
[ "$(cat "$D/ntstate/pid")" = "$NPID" ] || fail "transcript-less watcher pid file mismatch"
rc=$(run_snippet "$D/ntstate")
[ "$rc" -eq 0 ] || fail "snippet exited rc=$rc on a transcript-less watcher pid"
deadline=$((SECONDS + 30))
while kill -0 "$NPID" 2>/dev/null; do
  [ "$SECONDS" -lt "$deadline" ] || fail "validated kill refused a live transcript-less watcher $NPID"
  sleep 0.2
done

echo "kill-handle-smoke: ok"
