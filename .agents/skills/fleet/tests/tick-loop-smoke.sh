#!/bin/bash
# Guards the SKILL.md step-6 tick-loop snippet by EXTRACTING it from the doc
# (so doc drift cannot silently break it): seeds must let the engagement's
# FIRST wake break the slice loop, anchored to the watcher state dir, with no
# stderr noise. sleep is stubbed to append one wake; early break => exactly
# one stub call. The extracted snippet runs under bash -eu -o pipefail from a
# file, so an errexit-hostile edit reds it.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../../../.."
SKILL=.agents/skills/fleet/SKILL.md
fail() { echo "FAIL: $1" >&2; exit 1; }

RAW=$(python3 - "$SKILL" <<'PY'
import re, sys
text = open(sys.argv[1]).read()
blocks = re.findall(r'```sh\n(.*?)```', text, re.S)
picks = [b for b in blocks if 'seq 20' in b]
assert len(picks) == 1, f"expected exactly one tick-loop block, got {len(picks)}"
print(picks[0])
PY
)
[ -n "$RAW" ] || fail "tick-loop snippet not found in SKILL.md"
grep -q 'S=/tmp/fleet-<engagement>' <<<"$RAW" || fail "snippet lost its S= state-dir anchor line (must live inside the fence)"
SNIPPET=$(grep -v '<engagement>' <<<"$RAW" | sed 's/^   //')
# shellcheck disable=SC2016  # literal $S is the point: the snippet must reference it
grep -q '"\$S/wake.txt"' <<<"$SNIPPET" || fail "snippet not anchored to the state dir"

D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
mkdir -p "$D/state" "$D/cwd"
export S="$D/state"; export D
sleep() { printf 'x wake test\n' >>"$S/wake.txt"; echo x >>"$D/sleeps"; }
export -f sleep >/dev/null 2>&1 || true
printf '%s\n' "$SNIPPET" >"$D/snippet.sh"
rc=0; ( cd "$D/cwd" && bash -eu -o pipefail "$D/snippet.sh" ) 2>"$D/err" || rc=$?
[ "$rc" -eq 0 ] || fail "snippet exited rc=$rc ($(head -1 "$D/err" 2>/dev/null))"
[ -s "$D/err" ] && fail "snippet wrote stderr: $(head -1 "$D/err")"
[ "$(wc -l <"$D/sleeps")" -eq 1 ] || fail "loop did not break on the first wake ($(wc -l <"$D/sleeps") sleeps)"
[ -e "$D/cwd/wake.txt" ] && fail "snippet touched a cwd-relative wake.txt"
echo "tick-loop-smoke: ok"
