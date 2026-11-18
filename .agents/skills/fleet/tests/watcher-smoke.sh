#!/bin/bash
# Offline smoke for .agents/skills/fleet/watcher.sh: fake the watcher's
# actual inputs — wake-source IDs (via FLEET_LIST_IDS_CMD) and transcript
# mtimes. No external service, no panes, no network. Deadline-polled: no
# assertion trusts a fixed sleep, so a loaded runner cannot flake it.
# hb() reads the heartbeat content (portable; the watcher writes a fresh
# timestamp per tick) instead of a GNU-only stat form; age20m() replaces
# a GNU-only touch form with a BSD/GNU dual arm; paths are re-rooted.
# Case 5 keeps the stronger arm (staleness must keep running when no
# wake source is configured).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../../../.."
WATCHER=.agents/skills/fleet/watcher.sh
D=$(mktemp -d)
trap 'kill "$(cat "$D/state/pid" 2>/dev/null || echo 999999999)" 2>/dev/null || true; rm -rf "$D"' EXIT
fail() { echo "FAIL: $1" >&2; exit 1; }

age20m() { # set mtime to 20 minutes ago — BSD date -v first, GNU date -d fallback
  touch -t "$(date -v-20M +%Y%m%d%H%M.%S 2>/dev/null || date -d '20 minutes ago' +%Y%m%d%H%M.%S)" "$1"
}

poll() { # poll <what> <cond-fn> [args...] — re-evaluates every 0.2s, 30s deadline
  local what=$1 deadline=$((SECONDS + 30)); shift
  until "$@"; do [ "$SECONDS" -lt "$deadline" ] || fail "timeout: $what"; sleep 0.2; done
}
wakes() { grep -c -- "$1" "$D/state/wake.txt"; }
wake_seen() { [ "$(wakes "$1")" -ge 1 ]; }
wake_count_is() { [ "$(wakes "$1")" = "$2" ]; }
absent() { [ ! -e "$1" ]; }
hb() { cat "$D/state/heartbeat" 2>/dev/null || echo none; }
ticks() { # observe N further heartbeat refreshes (proves N full watcher ticks)
  local n=$1 last cur deadline=$((SECONDS + 30)); last=$(hb)
  while [ "$n" -gt 0 ]; do
    [ "$SECONDS" -lt "$deadline" ] || fail "timeout waiting for $n ticks"
    sleep 0.2; cur=$(hb)
    if [ "$cur" != "$last" ]; then last=$cur; n=$((n - 1)); fi
  done
}

printf 'wake-a\nwake-b\n' >"$D/ids"
touch "$D/fresh.jsonl"; touch "$D/old.jsonl"; age20m "$D/old.jsonl"
mkdir -p "$D/state"; : >"$D/state/wake.txt"

FLEET_LIST_IDS_CMD="cat $D/ids" FLEET_TICK_SECS=1 FLEET_STALE_SECS=600 \
  sh "$WATCHER" "$D/state" "$D/fresh.jsonl" "$D/old.jsonl" &
poll "wake a" wake_seen ' wake wake-a$'
poll "wake b" wake_seen ' wake wake-b$'
poll "stale wake" wake_seen " stale $D/old.jsonl$"
ticks 2 # dedup must hold across further full ticks
# 1. seen-set: each wake ID wakes exactly once
wake_count_is ' wake wake-a$' 1 || fail "wake id a not exactly once"
wake_count_is ' wake wake-b$' 1 || fail "wake id b not exactly once"
# 2. staleness: old fires once per episode; fresh never fires
wake_count_is " stale $D/old.jsonl$" 1 || fail "stale not exactly once"
wake_count_is " stale $D/fresh.jsonl$" 0 || fail "fresh transcript flagged stale"
# 3. episode reset: freshen (marker clears), re-stale -> exactly one NEW wake
MARK="$D/state/stale.$(printf '%s' "$D/old.jsonl" | cksum | cut -d' ' -f1)"
touch "$D/old.jsonl"; poll "episode marker cleared" absent "$MARK"
age20m "$D/old.jsonl"
poll "stale re-fire" wake_count_is " stale $D/old.jsonl$" 2

kill "$(cat "$D/state/pid")"; wait 2>/dev/null || true
# 4. durable seen-set AND a warm restart still consumes NEW ids exactly once
printf 'wake-c\n' >>"$D/ids"
FLEET_LIST_IDS_CMD="cat $D/ids" FLEET_TICK_SECS=1 \
  sh "$WATCHER" "$D/state" "$D/fresh.jsonl" &
poll "restart sees new id" wake_seen ' wake wake-c$'
ticks 2
wake_count_is ' wake wake-c$' 1 || fail "new id after restart not exactly once"
wake_count_is ' wake ' 3 || fail "restart duplicated wake wakes"
kill "$(cat "$D/state/pid")"; wait 2>/dev/null || true

# 5. no wake source: with NO lister override the watcher must run NO
#    wake-source check — no wake lines and no wake-source.err — while
#    the STALENESS check keeps running (an aged transcript must still
#    wake in the default mode, or a mutant that skips the whole tick
#    body when no wake source is set would pass this smoke)
touch "$D/old5.jsonl"; age20m "$D/old5.jsonl"
WAKE_BASELINE=$(wakes ' wake ')
FLEET_TICK_SECS=1 FLEET_STALE_SECS=600 \
  sh "$WATCHER" "$D/state" "$D/fresh.jsonl" "$D/old5.jsonl" &
poll "no-source stale wake" wake_seen " stale $D/old5.jsonl$"
ticks 2
absent "$D/state/wake-source.err" || fail "watcher without a wake source wrote wake-source.err"
wake_count_is ' wake ' "$WAKE_BASELINE" || fail "watcher without a wake source emitted wake lines"
wake_count_is " stale $D/old5.jsonl$" 1 || fail "no-source stale not exactly once"
kill "$(cat "$D/state/pid")"; wait 2>/dev/null || true

# 6. crash-window ordering: wake append precedes seen append, so an id whose
#    wake write DIES is retried (not seen-forever). Read-only wake.txt kills
#    the watcher mid-window; after restore + relaunch the id must still wake.
cat "$D/ids" >"$D/ids6"; printf 'wake-d\n' >>"$D/ids6"
chmod a-w "$D/state/wake.txt"
FLEET_LIST_IDS_CMD="cat $D/ids6" FLEET_TICK_SECS=1 \
  sh "$WATCHER" "$D/state" "$D/fresh.jsonl" &
W=$!
poll "watcher dies on read-only wake.txt" sh -c "! kill -0 $W 2>/dev/null"
wait 2>/dev/null || true
chmod u+w "$D/state/wake.txt"
FLEET_LIST_IDS_CMD="cat $D/ids6" FLEET_TICK_SECS=1 \
  sh "$WATCHER" "$D/state" "$D/fresh.jsonl" &
poll "crash-window id wakes after restart" wake_seen ' wake wake-d$'
kill "$(cat "$D/state/pid")"; wait 2>/dev/null || true

# 7. wake-source failure surfaces: a failing lister appends a timestamped
#    line per failure to wake-source.err and, after N consecutive failures,
#    exactly one wake reason; recovery clears the episode so the next
#    outage wakes again. No silent broken wake source anymore.
WAKEFAIL_BASELINE=$(wakes 'wake-source-failing' || true)
FLEET_LIST_IDS_CMD="false" FLEET_TICK_SECS=1 \
  sh "$WATCHER" "$D/state" "$D/fresh.jsonl" &
poll "wake-source.err appears" test -s "$D/state/wake-source.err"
poll "wake-source-failing wake" wake_seen 'wake-source-failing 3 consecutive failures'
ticks 2 # episode marker must hold across further full ticks
wake_count_is 'wake-source-failing 3 consecutive failures' $((WAKEFAIL_BASELINE + 1)) || fail "wake-source-failing did not fire exactly once at the threshold"
[ "$(wc -l <"$D/state/wake-source.err")" -ge 2 ] || fail "wake-source.err missing per-failure lines"
grep -qE '^[0-9]{4}-[0-9]{2}-[0-9]{2}T' "$D/state/wake-source.err" || fail "wake-source.err lines not timestamped"
kill "$(cat "$D/state/pid")"; wait 2>/dev/null || true
# recovery: a healthy lister clears the episode; a NEW outage wakes again
FLEET_LIST_IDS_CMD="cat $D/ids" FLEET_TICK_SECS=1 \
  sh "$WATCHER" "$D/state" "$D/fresh.jsonl" &
ticks 2
kill "$(cat "$D/state/pid")"; wait 2>/dev/null || true
absent "$D/state/wake-source-failing" || fail "recovery did not clear the wake-source-failing episode"
FLEET_LIST_IDS_CMD="false" FLEET_TICK_SECS=1 \
  sh "$WATCHER" "$D/state" "$D/fresh.jsonl" &
poll "second outage wakes again" wake_count_is 'wake-source-failing 3 consecutive failures' $((WAKEFAIL_BASELINE + 2))
kill "$(cat "$D/state/pid")"; wait 2>/dev/null || true

# 8. lister error with parseable stdout still surfaces: the watcher must
#    branch on the LIST command's own exit, so a failing lister that
#    still prints `[]` counts as failure, not quiet.
printf '#!/bin/sh\nprintf "[]\\n"\nexit 1\n' >"$D/lister.sh"
chmod +x "$D/lister.sh"
# fresh episode: healthy ticks clear the carried-over outage state first
FLEET_LIST_IDS_CMD="cat $D/ids" FLEET_TICK_SECS=1 \
  sh "$WATCHER" "$D/state" "$D/fresh.jsonl" &
ticks 2
kill "$(cat "$D/state/pid")"; wait 2>/dev/null || true
LISTFAIL_BASELINE=$(wakes 'wake-source-failing' || true)
ERR_BASELINE=$(wc -l <"$D/state/wake-source.err")
err_grew() { [ "$(wc -l <"$D/state/wake-source.err")" -gt "$1" ]; }
FLEET_LIST_IDS_CMD="sh $D/lister.sh" FLEET_TICK_SECS=1 \
  sh "$WATCHER" "$D/state" "$D/fresh.jsonl" &
poll "failing lister with parseable stdout appends wake-source.err" err_grew "$ERR_BASELINE"
poll "failing lister wakes wake-source-failing" wake_count_is 'wake-source-failing 3 consecutive failures' $((LISTFAIL_BASELINE + 1))
kill "$(cat "$D/state/pid")"; wait 2>/dev/null || true

# 9. launch guard: a transcript argument that is not an existing regular
#    file refuses to start — loudly, naming the argument, BEFORE recording
#    pid or heartbeat (a shell that passes N paths as one unsplit
#    argument must never run as a silently-stale mega-path).
TIMEOUT_BIN="$(command -v timeout || command -v gtimeout)" || fail "neither timeout nor gtimeout on PATH (watcher dependency)"
G="$D/guard-state"
rc=0
"$TIMEOUT_BIN" 10 sh "$WATCHER" "$G" "$D/fresh.jsonl $D/old.jsonl" 2>"$D/guard.err" || rc=$?
# Pin the exact exit code (3), not merely nonzero — a mutated exit code
# must fail this case.
[ "$rc" -eq 3 ] || fail "guard must exit 3 on a non-file transcript argument (got rc=$rc)"
grep -q 'not an existing regular file' "$D/guard.err" || fail "guard did not say why it refused"
grep -qF "$D/fresh.jsonl $D/old.jsonl" "$D/guard.err" || fail "guard did not name the offending argument"
[ ! -e "$G/pid" ] || fail "guard recorded pid before refusing"
[ ! -e "$G/heartbeat" ] || fail "guard recorded heartbeat before refusing"

# 9b. the guard requires a REGULAR file, not merely an existing path: a
#     directory argument (whose mtime would silently stand in for a worker's
#     staleness) must also refuse with exit 3 and no pid — pins the -f (not -e)
#     half of the guard.
rcd=0
"$TIMEOUT_BIN" 10 sh "$WATCHER" "$D/guard-dir-state" "$D" 2>"$D/guard-dir.err" || rcd=$?
[ "$rcd" -eq 3 ] || fail "guard must exit 3 on a directory transcript argument (got rc=$rcd)"
[ ! -e "$D/guard-dir-state/pid" ] || fail "guard recorded pid for a directory argument"

echo "watcher-smoke: ok"
