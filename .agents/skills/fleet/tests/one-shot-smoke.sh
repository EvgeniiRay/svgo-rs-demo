#!/bin/bash
# Runs SKILL.md's committed one-shot payload fence with a stub tmux:
# a stale fire (pane back at a shell, or a different command) must exit 0
# with no send-keys; a live match must emit the steering send sequence.
# Proves the wrong-pane guard is fail-closed, not prose.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../../../.."
SKILL=.agents/skills/fleet/SKILL.md
fail() { echo "FAIL: $1" >&2; exit 1; }

RAW=$(python3 - "$SKILL" <<'PY'
import re, sys
text = open(sys.argv[1]).read()
blocks = re.findall(r'```sh\n(.*?)```', text, re.S)
picks = [b for b in blocks if 'pane_pid' in b and 'send-keys' in b]
assert len(picks) == 1, f"expected exactly one one-shot payload block, got {len(picks)}"
print(picks[0])
PY
)
[ -n "$RAW" ] || fail "one-shot payload not found in SKILL.md"
# shellcheck disable=SC2001 # strip the 3-space markdown-fence indent tick-loop-smoke.sh also keeps
SNIPPET=$(sed 's/^   //' <<<"$RAW")

D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
export D
printf '%s\n' "$SNIPPET" >"$D/payload.sh"

run_case() {  # $1 = stubbed live reading, $2 = SOCK value; asserts via $D/calls + $D/err
  export STUB_LIVE="$1" STUB_SOCK="$2"
  : >"$D/calls"
  tmux() {
    if [ "${1:-}" != "-S" ] || [ -z "$STUB_SOCK" ] || [ "${2:-}" != "$STUB_SOCK" ]; then
      echo "wrong socket: $*" >&2; return 1
    fi
    shift 2
    case "$1" in
      display) [ "$*" = "display -p -t $PANE #{pane_pid} #{pane_current_command}" ] \
                 || { echo "display read off the armed pane: $*" >&2; return 1; }
               printf '%s\n' "$STUB_LIVE" ;;
      send-keys) printf '%s\n' "$*" >>"$D/calls" ;;
      *) echo "unexpected tmux call: $*" >&2; return 1 ;;
    esac
  }
  sleep() { echo "sleep $*" >>"$D/calls"; }
  export -f tmux sleep >/dev/null 2>&1 || true
  export PANE='%3' ARMED='1234 codex' SOCK="$STUB_SOCK"
  ( bash -eu -o pipefail "$D/payload.sh" ) 2>"$D/err" \
    || fail "payload exited nonzero for live='$1' ($(head -1 "$D/err" 2>/dev/null))"
  [ -s "$D/err" ] && fail "payload wrote stderr: $(head -1 "$D/err")"
  return 0
}

expect_calls() {  # exact call order; socket routing is asserted by the stub guards
  diff -u - "$D/calls" <<'EOF' || fail "steering send sequence mismatch"
send-keys -t %3 C-u
send-keys -t %3 -l tick: scheduled self-check, run one full tick
sleep 1
send-keys -t %3 Enter
EOF
}

run_case "9999 sh" "/sock"      # pane recycled to a shell
[ -s "$D/calls" ] && fail "stale fire typed (shell case)"
run_case "1234 vim" "/sock"     # same pane, different command
[ -s "$D/calls" ] && fail "stale fire typed (command case)"
run_case "1234 codex" "/sock"   # live match on parent socket: exact sequence
expect_calls
run_case "1234 codex" "/other"  # live match on a second socket: value flows through
expect_calls
: >"$D/calls"                   # unset socket must trip the :? guard, loudly
( unset SOCK; export PANE='%3' ARMED='1234 codex'; bash -eu "$D/payload.sh" ) 2>"$D/err" \
  && fail "payload ran with no socket baked"
grep -q 'SOCK: parameter null or not set' "$D/err" || fail "unset SOCK did not trip the :? guard"
[ -s "$D/calls" ] && fail "typed with no socket baked"
echo "one-shot-smoke: ok"
