#!/bin/sh
# Fleet watcher — one parse-free check, plus an optional second one.
# It never inspects pane or transcript CONTENT: (1) per-worker transcript
# mtime staleness. Wake reasons append to <state-dir>/wake.txt; a heartbeat
# file proves the watcher itself is alive; <state-dir>/pid is the kill handle
# (the watcher records its own PID — do not trust a launcher's $!).
# (2) Optional wake source: when FLEET_LIST_IDS_CMD is set, each tick also
# runs that command (one wake ID per line, plain text — no parsing) and
# wakes once per unseen ID. There is NO default wake source: with no
# override the watcher calls no external service at all, and a failing
# wake source is never silent: every failure appends a timestamped
# line to <state-dir>/wake-source.err, and after
# FLEET_WAKE_FAIL_WAKE_AFTER consecutive failures one wake-source-failing
# wake fires per episode. The wake-source check is bounded by
# FLEET_WAKE_TIMEOUT_SECS: a hung wake source degrades to
# wake-source-failing accounting, never a wedged tick loop.
# Usage: watcher.sh <state-dir> <transcript-file>...
# Env: FLEET_STALE_SECS (600) FLEET_TICK_SECS (60)
#      FLEET_LIST_IDS_CMD — optional wake-ID lister (one ID per line).
#      FLEET_WAKE_FAIL_WAKE_AFTER (3) — consecutive wake-source failures
#      before one wake-source-failing wake per episode.
#      FLEET_WAKE_TIMEOUT_SECS (30) — bound on one wake-source check
#      (TERM, then KILL 5s later); keep FLEET_WAKE_TIMEOUT_SECS + 5 below
#      FLEET_TICK_SECS.
set -eu
TIMEOUT=$(command -v timeout || command -v gtimeout) || { echo "watcher.sh: GNU timeout (coreutils) required" >&2; exit 1; }
STATE=$1; shift
# Launch guard: refuse a transcript argument that is not an existing
# regular file BEFORE recording pid/heartbeat — a shell that passes N
# paths as one unsplit argument must never run as a silently-always-stale
# mega-path.
for _t in "$@"; do
  [ -f "$_t" ] || { echo "watcher.sh: not an existing regular file (transcript): $_t" >&2; exit 3; }
done
STALE=${FLEET_STALE_SECS:-600}; TICK=${FLEET_TICK_SECS:-60}
WAKE_TIMEOUT=${FLEET_WAKE_TIMEOUT_SECS:-30}
mkdir -p "$STATE"; : >>"$STATE/seen"; printf '%s\n' "$$" >"$STATE/pid"
while :; do
  date -u +%FT%TZ >"$STATE/heartbeat"
  if [ -n "${FLEET_LIST_IDS_CMD:-}" ]; then
    if "$TIMEOUT" -k 5 "$WAKE_TIMEOUT" sh -c "$FLEET_LIST_IDS_CMD" >"$STATE/.ids.tmp" 2>/dev/null; then
      rm -f "$STATE/wake-source-fail-count" "$STATE/wake-source-failing"
      while IFS= read -r id; do
        [ -n "$id" ] || continue
        grep -qxF "$id" "$STATE/seen" && continue
        printf '%s wake %s\n' "$(date -u +%FT%TZ)" "$id" >>"$STATE/wake.txt"
        printf '%s\n' "$id" >>"$STATE/seen"
      done <"$STATE/.ids.tmp"
    else
      rc=$?
      fails=$(( $(cat "$STATE/wake-source-fail-count" 2>/dev/null || echo 0) + 1 ))
      printf '%s\n' "$fails" >"$STATE/wake-source-fail-count"
      printf '%s wake-source check failed (rc=%s, %s consecutive)\n' "$(date -u +%FT%TZ)" "$rc" "$fails" >>"$STATE/wake-source.err"
      if [ "$fails" -ge "${FLEET_WAKE_FAIL_WAKE_AFTER:-3}" ] && [ ! -e "$STATE/wake-source-failing" ]; then
        printf '%s wake-source-failing %s consecutive failures\n' "$(date -u +%FT%TZ)" "$fails" >>"$STATE/wake.txt"
        : >"$STATE/wake-source-failing"
      fi
    fi
  fi
  now=$(date +%s)
  for t in "$@"; do
    mark="$STATE/stale.$(printf '%s' "$t" | cksum | cut -d' ' -f1)"
    if [ $((now - $(stat -c %Y "$t" 2>/dev/null || stat -f %m "$t" 2>/dev/null || echo 0))) -ge "$STALE" ]; then
      [ -e "$mark" ] || { printf '%s stale %s\n' "$(date -u +%FT%TZ)" "$t" >>"$STATE/wake.txt"; : >"$mark"; }
    else rm -f "$mark"; fi
  done
  sleep "$TICK"
done
