#!/bin/bash
# Hermetic fleet-herdr smoke: stub herdr, git, jq, uname, cygpath, and
# ssh; real git repos. Covers the setup fixes: multi-word prompts,
# kind-list empties, jq 1.6 empty replies, backslash record paths,
# wind-down merge gates, simulated Windows (Git Bash), CRLF-jq
# refusal, and both machine-list shapes. No network, no real Herdr.
set -u
cd "$(dirname "${BASH_SOURCE[0]}")/../../../../.."
CLI=.agents/skills/fleet/herdr/fleet-herdr
JQ_BIN=$(command -v jq); GIT_BIN=$(command -v git); UNAME_BIN=$(command -v uname)
export JQ_BIN GIT_BIN UNAME_BIN
fails=0
fail() { echo "FAIL: $1" >&2; fails=$((fails + 1)); }
pass() { echo "ok   $1"; }

D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
mkdir -p "$D/bin" "$D/winbin" "$D/jq16bin" "$D/crlfbin"

cat >"$D/bin/herdr" <<'STUB'
#!/bin/bash
printf 'ENV MSYS_NO_PATHCONV=%s TMPDIR=%s\n' "${MSYS_NO_PATHCONV:-<unset>}" "${TMPDIR:-<unset>}" >>"$STUB_LOG"
printf '%q ' "$@" >>"$STUB_LOG"; printf '\n' >>"$STUB_LOG"
case "$1 $2" in
  "workspace create") printf '{"result":{"workspace":{"workspace_id":"w9"},"tab":{"tab_id":"w9:t1"},"root_pane":{"pane_id":"w9:p1"}}}\n' ;;
  "worktree create")
    scwd=""; sbranch=""; sbase=""; spath=""
    while [ $# -gt 0 ]; do case "$1" in --cwd) scwd=$2; shift 2;; --branch) sbranch=$2; shift 2;; --base) sbase=$2; shift 2;; --path) spath=$2; shift 2;; *) shift;; esac; done
    git -C "$scwd" worktree add -q "$spath" -b "$sbranch" "$sbase" || exit 1
    # In the simulated-Windows case the git stub rewrote the path back to
    # the source repo, so also create the directory at the mapped path
    # Herdr would have created there; on a real host both are one dir.
    mkdir -p "$spath" 2>/dev/null || true
    printf '{"result":{"workspace":{"workspace_id":"wK"},"root_pane":{"pane_id":"wK:p9"}}}\n' ;;
  "pane split") printf '{"result":{"pane":{"pane_id":"w9:p2"}}}\n' ;;
  "pane move") printf '{"result":{"move_result":{"pane":{"pane_id":"w9:p2"}}}}\n' ;;
  "pane close") printf '{"result":{}}\n' ;;
  "agent start") if [ "${STUB_EMPTY_START:-0}" = 1 ]; then printf ''; else printf '{"result":{}}\n'; fi ;;
  "agent prompt") printf '{"result":{}}\n' ;;
  "agent get") printf '{"result":{"agent":{"agent_status":"idle","pane_id":"w9:p2"}}}\n' ;;
  "agent read") printf '{"result":{"text":"hello"}}\n' ;;
  "workspace close") if [ "${STUB_EMPTY_CLOSE:-0}" = 1 ]; then printf ''; else printf '{"result":{}}\n'; fi ;;
  "machine list")
    if [ "${STUB_MACHINE_SHAPE:-bare}" = object ]; then
      printf '{"result":{"machines":[{"id":"m1","label":"studio","target":"studio","session":"default","enabled":true}]}}\n'
    else
      printf '[{"id":"m1","label":"studio","target":"studio","session":"default","enabled":true}]\n'
    fi ;;
  *) printf '{"error":{"code":"stub_missing","message":"no stub for %s %s"}}\n' "$1" "$2" >&2; exit 1 ;;
esac
STUB
cat >"$D/bin/ssh" <<'STUB'
#!/bin/bash
printf '%q ' "$@" >>"$STUB_LOG"; printf '\n' >>"$STUB_LOG"
exit 0
STUB
chmod +x "$D/bin/herdr" "$D/bin/ssh"

# uname stub: MINGW under Git Bash simulation, real uname otherwise.
cat >"$D/winbin/uname" <<'STUB'
#!/bin/bash
if [ "${1:-}" = -s ]; then printf 'MINGW64_NT-10.0\n'; else exec "$UNAME_BIN" "$@"; fi
STUB
# cygpath stub: map only the configured repo prefix (and /c/...) to C:/...,
# leave /tmp and everything else alone, as the real one would for TMPDIR.
cat >"$D/winbin/cygpath" <<'STUB'
#!/bin/bash
p=""; for a in "$@"; do p="$a"; done
if [ -n "${CYGPATH_FROM:-}" ]; then case "$p" in "$CYGPATH_FROM"*) printf '%s%s\n' "${CYGPATH_TO:-C:/winrepo}" "${p#$CYGPATH_FROM}"; exit 0;; esac; fi
case "$p" in /c/*) printf 'C:/%s\n' "${p#/c/}";; *) printf '%s\n' "$p";; esac
STUB
# git stub: undo the C:/ prefix the script stored, so real git on this host
# can open the repo; a no-op when no mapping is configured.
cat >"$D/winbin/git" <<'STUB'
#!/bin/bash
args=()
for a in "$@"; do
  if [ -n "${GIT_FROM:-}" ]; then case "$a" in "$GIT_FROM"*) args+=("$GIT_TO${a#$GIT_FROM}");; *) args+=("$a");; esac; else args+=("$a"); fi
done
exec "$GIT_BIN" "${args[@]}"
STUB
chmod +x "$D/winbin/uname" "$D/winbin/cygpath" "$D/winbin/git"

# jq stub mimicking jq 1.6: empty input under -e exits 0; else real jq.
cat >"$D/jq16bin/jq" <<'STUB'
#!/bin/bash
for a in "$@"; do [ -f "$a" ] && exec "$JQ_BIN" "$@"; done
input=$(cat)
has_e=0; for a in "$@"; do [ "$a" = "-e" ] && has_e=1; done
if [ -z "$input" ] && [ "$has_e" = 1 ]; then exit 0; fi
printf '%s' "$input" | exec "$JQ_BIN" "$@"
STUB
# jq stub with no -b that writes CRLF: the Windows refusal case.
cat >"$D/crlfbin/jq" <<'STUB'
#!/bin/bash
for a in "$@"; do [ "$a" = "-b" ] && exit 1; done
if [ "$1" = "-n" ]; then printf '1\r\n'; exit 0; fi
exec "$JQ_BIN" "$@"
STUB
chmod +x "$D/jq16bin/jq" "$D/crlfbin/jq"
cp "$D/winbin/uname" "$D/crlfbin/uname"; chmod +x "$D/crlfbin/uname"

mk_repo() { # $1 = tag; sets REPO and BASE
  REPO="$D/repo-$1"
  git init -q -b main "$REPO" && git -C "$REPO" config user.email t@t && git -C "$REPO" config user.name t
  echo hi >"$REPO/f.txt" && git -C "$REPO" add . && git -C "$REPO" commit -qm init
  BASE=$(git -C "$REPO" rev-parse HEAD)
}
run_up() { # $@ = fleet-herdr up args; HERDR_ENV=1, scratch state
  export STUB_LOG="$D/calls" HERDR_ENV=1 FLEET_HERDR_STATE_ROOT="$D/state-$1"
  shift
  : >"$D/calls"
  HERDR_ENV=1 "$CLI" "$@"
}

# 1. A multi-word prompt survives the top-level argument handling intact.
mk_repo prompt; rm -rf "$D/state-prompt"
export PATH="$D/bin:/usr/bin:/bin"
if run_up prompt up --engagement prompt --workers 1 --kind muse --repo "$REPO" --base "$BASE" --prompt "do the thing" >/dev/null 2>"$D/err"; then
  grep -qF 'do\ the\ thing' "$D/calls" && pass "multi-word prompt preserved" || fail "multi-word prompt mangled: $(grep 'agent prompt' "$D/calls")"
else fail "up failed for multi-word prompt: $(head -2 "$D/err")"; fi

# 2. Empty kind entries are rejected: leading, trailing, and doubled commas.
i=0
for k in 'muse,' 'muse,,codex' ',muse'; do
  i=$((i + 1)); mk_repo "kind$i"; rm -rf "$D/state-kind$i"
  export PATH="$D/bin:/usr/bin:/bin"
  if HERDR_ENV=1 FLEET_HERDR_STATE_ROOT="$D/state-kind$i" "$CLI" up --engagement "kind$i" --workers 1 --kind "$k" --repo "$REPO" --base "$BASE" >/dev/null 2>"$D/err"; then
    fail "kind '$k' accepted"
  elif [ -f "$D/state-kind$i/fleet-kind$i/herdr.json" ]; then fail "kind '$k' left a record behind";
  else pass "kind '$k' rejected"; fi
done

# 3. Empty Herdr replies do not fail under jq 1.6 empty-input behaviour.
mk_repo jq16; rm -rf "$D/state-jq16"
export PATH="$D/jq16bin:$D/bin:/usr/bin:/bin" STUB_EMPTY_START=1 STUB_EMPTY_CLOSE=1
if HERDR_ENV=1 FLEET_HERDR_STATE_ROOT="$D/state-jq16" "$CLI" up --engagement jq16 --workers 1 --kind muse --repo "$REPO" --base "$BASE" --prompt "hi" >/dev/null 2>"$D/err"; then
  pass "empty Herdr replies succeed (jq 1.6 behaviour)"
else fail "empty Herdr replies failed: $(head -2 "$D/err")"; fi
unset STUB_EMPTY_START STUB_EMPTY_CLOSE

# 4. A repo path with a backslash stays valid JSON in the record.
BS_DIR="$D/bs/a\\b"
mkdir -p "$BS_DIR" && git init -q -b main "$BS_DIR" && git -C "$BS_DIR" config user.email t@t && git -C "$BS_DIR" config user.name t
echo hi >"$BS_DIR/f.txt" && git -C "$BS_DIR" add . && git -C "$BS_DIR" commit -qm init
BS_BASE=$(git -C "$BS_DIR" rev-parse HEAD); rm -rf "$D/state-bs"
export PATH="$D/bin:/usr/bin:/bin"
if HERDR_ENV=1 FLEET_HERDR_STATE_ROOT="$D/state-bs" "$CLI" up --engagement bs --workers 1 --kind muse --repo "$BS_DIR" --base "$BS_BASE" --prompt "hi" >/dev/null 2>"$D/err" \
  && HERDR_ENV=1 FLEET_HERDR_STATE_ROOT="$D/state-bs" "$CLI" status --engagement bs >/dev/null 2>"$D/err2"; then
  [ "$("$JQ_BIN" -r .repo "$D/state-bs/fleet-bs/herdr.json")" = "$BS_DIR" ] && pass "backslash repo path round-trips through the record" || fail "record repo mismatch"
else fail "backslash repo path failed: $(head -2 "$D/err" "$D/err2")"; fi

# 5. Wind-down gates: merged into origin/main passes; unmerged refuses.
mk_winddown() { # $1 tag, $2 = origin|none; sets WD_REPO/WD_WT
  WD_REPO="$D/wd-$1"; git init -q -b main "$WD_REPO" && git -C "$WD_REPO" config user.email t@t && git -C "$WD_REPO" config user.name t
  echo a >"$WD_REPO/f.txt" && git -C "$WD_REPO" add . && git -C "$WD_REPO" commit -qm A
  git -C "$WD_REPO" checkout -qb feat && echo b >>"$WD_REPO/f.txt" && git -C "$WD_REPO" commit -qam B
  FEAT=$(git -C "$WD_REPO" rev-parse HEAD); git -C "$WD_REPO" checkout -q main
  [ "$2" = origin ] && git -C "$WD_REPO" update-ref refs/remotes/origin/main "$FEAT"
  WD_WT="$WD_REPO/.worktrees/demo-w1"
  git -C "$WD_REPO" worktree add -q "$WD_WT" -b demo-w1 "$FEAT"
  mkdir -p "$D/state-wd-$1/fleet-demo"
  printf '{"engagement":"demo","workspace_id":"w9","tab_id":"w9:t1","root_pane":"w9:p1","kind":"muse","kinds":["muse"],"agent_args":[],"env":[],"preference":null,"repo":"%s","base":"main","cwd_rel":"","workers":[{"lane":"w1","agent":"demo-w1","pane_id":"w9:p2","worktree":"%s","branch":"demo-w1","kind":"muse","agent_args":[],"env":[]}]}' "$WD_REPO" "$WD_WT" >"$D/state-wd-$1/fleet-demo/herdr.json"
}
export PATH="$D/bin:/usr/bin:/bin"
mk_winddown merged origin
if HERDR_ENV=1 FLEET_HERDR_STATE_ROOT="$D/state-wd-merged" "$CLI" wind-down --engagement demo >/dev/null 2>"$D/err" && [ ! -e "$WD_WT" ]; then
  pass "wind-down proceeds when merged into origin/main"
else fail "wind-down refused origin-merged lane: $(head -2 "$D/err")"; fi
mk_winddown unmerged none
if HERDR_ENV=1 FLEET_HERDR_STATE_ROOT="$D/state-wd-unmerged" "$CLI" wind-down --engagement demo >/dev/null 2>"$D/err"; then
  fail "wind-down proceeded for an unmerged lane"
elif [ -e "$WD_WT" ]; then pass "wind-down refuses unmerged lane and keeps its worktree";
else fail "wind-down refused but removed the worktree"; fi
git -C "$D/wd-unmerged" worktree remove --force "$D/wd-unmerged/.worktrees/demo-w1" 2>/dev/null || true

# 6. Simulated Windows (Git Bash): MSYS guards on, paths in C:/ form. A
# literal C:/ root cannot be stat-ed on this host, so the mapped path is a
# real directory containing a C:/winrepo segment; git calls are rewritten
# back to the source repo by the git stub, as the native tools would be.
mk_repo win; SRC_REPO="$D/winrepo-src"; git clone -q "$REPO" "$SRC_REPO"
WIN_MAPPED="$D/wintest/C:/winrepo"; mkdir -p "$D/wintest/C:"; git clone -q "$REPO" "$WIN_MAPPED"
rm -rf "$D/state-win"
export PATH="$D/winbin:$D/bin:/usr/bin:/bin" CYGPATH_FROM="$SRC_REPO" CYGPATH_TO="$WIN_MAPPED" GIT_FROM="$WIN_MAPPED" GIT_TO="$SRC_REPO"
if HERDR_ENV=1 FLEET_HERDR_STATE_ROOT="$D/state-win" "$CLI" up --engagement win --workers 1 --kind muse --repo "$SRC_REPO" --base "$BASE" --prompt "hi" >/dev/null 2>"$D/err"; then
  grep -q 'MSYS_NO_PATHCONV=1' "$D/calls" && grep -qF 'C:/winrepo' "$D/calls" && [ "$("$JQ_BIN" -r .repo "$D/state-win/fleet-win/herdr.json")" = "$WIN_MAPPED" ] \
    && pass "simulated Windows: MSYS guards, C:/ paths, record" || fail "simulated Windows assertions failed: $(grep 'ENV\|workspace create' "$D/calls" | head -3)"
else fail "simulated Windows up failed: $(head -3 "$D/err")"; fi
unset CYGPATH_FROM CYGPATH_TO GIT_FROM GIT_TO

# 7. A jq that writes CRLF without -b is refused on Windows with the fix.
export PATH="$D/crlfbin:$D/winbin:$D/bin:/usr/bin:/bin" JQ_BIN="$JQ_BIN"
if HERDR_ENV=1 "$CLI" --version >/dev/null 2>"$D/err"; then fail "CRLF jq was accepted on simulated Windows";
elif grep -q 'winget install jqlang.jq' "$D/err"; then pass "CRLF jq refused with install command";
else fail "CRLF jq refusal message wrong: $(head -2 "$D/err")"; fi

# 8. Both machine-list shapes resolve a saved machine for --machine.
export PATH="$D/bin:/usr/bin:/bin"
for shape in bare object; do
  export STUB_MACHINE_SHAPE="$shape"; : >"$D/calls"
  if HERDR_ENV=1 "$CLI" --machine studio status --engagement nope >/dev/null 2>"$D/err"; then
    grep -q 'BatchMode=yes' "$D/calls" && pass "machine list ($shape shape) resolves and streams over ssh" || fail "machine list ($shape): ssh not called"
  else fail "machine list ($shape) failed: $(head -2 "$D/err")"; fi
done

# 9. --lane-prompts: per-lane prompts ride with up; bad manifests cost nothing.
export PATH="$D/bin:/usr/bin:/bin"
mk_repo lp; rm -rf "$D/state-lp"
printf '%s' '{"w1": "lane one task", "w2": "lane two task"}' >"$D/prompts.json"
: >"$D/calls"
if HERDR_ENV=1 FLEET_HERDR_STATE_ROOT="$D/state-lp" "$CLI" up --engagement lp --workers 2 --kind muse --repo "$REPO" --base "$BASE" --prompt "shared fallback" --lane-prompts "$D/prompts.json" >/dev/null 2>"$D/err" \
  && grep -qF 'lane\ one\ task' "$D/calls" && grep -qF 'lane\ two\ task' "$D/calls" && ! grep -qF 'shared\ fallback' "$D/calls"; then
  pass "lane prompts dispatched per lane"
else fail "lane prompts not dispatched: $(head -2 "$D/err")"; fi
# Fallback: a lane with no entry, and an empty entry, get the shared prompt.
mk_repo lpf; rm -rf "$D/state-lpf"
printf '%s' '{"w1": "only one", "w2": ""}' >"$D/prompts-fallback.json"
: >"$D/calls"
if HERDR_ENV=1 FLEET_HERDR_STATE_ROOT="$D/state-lpf" "$CLI" up --engagement lpf --workers 2 --kind muse --repo "$REPO" --base "$BASE" --prompt "shared fallback" --lane-prompts "$D/prompts-fallback.json" >/dev/null 2>"$D/err" \
  && grep -qF 'only\ one' "$D/calls" && grep -cF 'shared\ fallback' "$D/calls" | grep -q '^1$'; then
  pass "lane without entry falls back to shared prompt"
else fail "lane fallback wrong: $(head -2 "$D/err")"; fi
# Inline JSON form dispatches the same way.
mk_repo lpj; rm -rf "$D/state-lpj"; : >"$D/calls"
if HERDR_ENV=1 FLEET_HERDR_STATE_ROOT="$D/state-lpj" "$CLI" up --engagement lpj --workers 1 --kind muse --repo "$REPO" --base "$BASE" --prompt "shared" --lane-prompts-json '{"w1": "inline task"}' >/dev/null 2>"$D/err" \
  && grep -qF 'inline\ task' "$D/calls" && ! grep -qF 'shared' "$D/calls"; then
  pass "lane-prompts-json dispatches inline"
else fail "lane-prompts-json failed: $(head -2 "$D/err")"; fi
# Bad manifests are rejected before any workspace is created.
i=0
for content in '{"w9": "no such lane"}' '{"w1": 42}' '{not json' '{"w1": "ok", "w3": "past workers"}'; do
  i=$((i + 1)); mk_repo "bad$i"; rm -rf "$D/state-bad$i"; : >"$D/calls"
  printf '%s' "$content" >"$D/prompts-bad.json"
  if HERDR_ENV=1 FLEET_HERDR_STATE_ROOT="$D/state-bad$i" "$CLI" up --engagement "bad$i" --workers 2 --kind muse --repo "$REPO" --base "$BASE" --prompt "shared" --lane-prompts "$D/prompts-bad.json" >/dev/null 2>"$D/err"; then
    fail "bad manifest accepted: $content"
  elif [ ! -f "$D/state-bad$i/fleet-bad$i/herdr.json" ] && ! grep -q 'workspace create' "$D/calls"; then
    pass "bad manifest rejected before creation: $content"
  else fail "bad manifest left state behind: $content"; fi
done
if HERDR_ENV=1 FLEET_HERDR_STATE_ROOT="$D/state-badx" "$CLI" up --engagement badx --workers 1 --kind muse --repo "$REPO" --base "$BASE" --lane-prompts "$D/no-such-file.json" >/dev/null 2>"$D/err"; then
  fail "missing lane-prompts file accepted"
else pass "missing lane-prompts file rejected"; fi

echo "herdr-smoke: $fails failure(s)"
[ "$fails" = 0 ]
