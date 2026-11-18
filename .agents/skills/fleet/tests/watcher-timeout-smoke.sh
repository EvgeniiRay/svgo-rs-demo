#!/bin/bash
# Offline smoke: a hung wake-source lister must degrade to "wake-source
# failing" accounting, never wedge the watcher's tick loop (heartbeat +
# staleness). No external service, no network. Deadline-polled: no
# assertion trusts a fixed sleep.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../../../.."
WATCHER=.agents/skills/fleet/watcher.sh
D=$(mktemp -d)
trap 'kill "$(cat "$D/state/pid" 2>/dev/null || echo 999999999)" 2>/dev/null || true; rm -rf "$D"' EXIT
fail() { # dump live watcher state first: a bare timeout line is undiagnosable after rm -rf
  echo "FAIL: $1" >&2
  date -u +%T
  cat "$D/state/heartbeat" 2>/dev/null || true
  tail -3 "$D/state/wake-source.err" 2>/dev/null || true
  local wpid; wpid=$(cat "$D/state/pid" 2>/dev/null || echo 999999999)
  ps -o pid,stat,etime,command -p "$wpid" 2>/dev/null || true
  for kid in $(pgrep -P "$wpid" 2>/dev/null || true); do
    ps -o pid,stat,etime,command -p "$kid" 2>/dev/null || true
  done
  exit 1
}

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
err_lines() { [ -f "$D/state/wake-source.err" ] && [ "$(wc -l <"$D/state/wake-source.err")" -ge "$1" ]; }
killed_line() { grep -q ' wake-source check failed (rc=137, ' "$D/state/wake-source.err" 2>/dev/null; }

printf 'exec sleep 30\n' >"$D/lister.sh" # hung wake source (rewritable per phase)
printf 'wake-a\n' >"$D/ids"
touch "$D/fresh.jsonl"; touch "$D/old.jsonl"; age20m "$D/old.jsonl"
mkdir -p "$D/state"; : >"$D/state/wake.txt"

FLEET_LIST_IDS_CMD="sh $D/lister.sh" FLEET_TICK_SECS=1 FLEET_STALE_SECS=600 \
  FLEET_WAKE_TIMEOUT_SECS=1 FLEET_WAKE_FAIL_WAKE_AFTER=2 \
  sh "$WATCHER" "$D/state" "$D/fresh.jsonl" "$D/old.jsonl" &
# 1. the tick loop survives a hung wake source: heartbeat keeps refreshing
ticks 2
# 2. every timed-out check appends a timestamped wake-source.err line
poll "wake-source.err timeout lines" err_lines 2
grep -qE '^[0-9T:.-]+Z wake-source check failed \(rc=[0-9]+, [0-9]+ consecutive\)$' "$D/state/wake-source.err" \
  || fail "wake-source.err line not timestamped: $(head -1 "$D/state/wake-source.err")"
# 3. one wake-source-failing wake per episode after N consecutive timeouts
poll "wake-source-failing wake" wake_seen ' wake-source-failing 2 consecutive failures$'
ticks 2
wake_count_is ' wake-source-failing ' 1 || fail "failing wake not exactly once per episode"
# 4. staleness coverage survives the wedge too
poll "stale wake under hung wake source" wake_seen " stale $D/old.jsonl$"

# 5. recovery clears the episode: a new outage wakes again
printf 'cat %s\n' "$D/ids" >"$D/lister.sh"
poll "healthy check consumes id" wake_seen ' wake wake-a$'
poll "episode cleared" absent "$D/state/wake-source-failing"
poll "fail count cleared" absent "$D/state/wake-source-fail-count"
printf 'exec sleep 30\n' >"$D/lister.sh"
poll "second outage wakes again" wake_count_is ' wake-source-failing ' 2

kill "$(cat "$D/state/pid")"; wait 2>/dev/null || true

# 6. a TERM-ignoring wake source is KILLed after the grace (rc=137), never wedges
rm -rf "$D/state"; mkdir -p "$D/state"; : >"$D/state/wake.txt"
printf '#!/bin/sh\ntrap "" TERM\nprintf "wake-a\\n"\nexec sleep 60\n' >"$D/lister.sh"
FLEET_LIST_IDS_CMD="sh $D/lister.sh" FLEET_TICK_SECS=1 FLEET_STALE_SECS=600 \
  FLEET_WAKE_TIMEOUT_SECS=1 FLEET_WAKE_FAIL_WAKE_AFTER=2 \
  sh "$WATCHER" "$D/state" "$D/fresh.jsonl" &
poll "kill-after wake-source.err line (rc=137)" killed_line
kill "$(cat "$D/state/pid")"; wait 2>/dev/null || true

# 7. the timeout guard fails closed: with no timeout on PATH the watcher
#    refuses to start (message, no state dir) instead of fake-outaging.
if PATH=/nonexistent /bin/sh "$WATCHER" "$D/guard" 2>"$D/guard.err"; then
  fail "watcher started without timeout on PATH"
fi
grep -q 'GNU timeout (coreutils) required' "$D/guard.err" \
  || fail "guard message missing: $(cat "$D/guard.err")"
[ ! -e "$D/guard" ] || fail "guard created state dir"

echo "watcher-timeout-smoke: ok"
