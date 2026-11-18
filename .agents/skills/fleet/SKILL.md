---
name: fleet
description: "Run a fleet of worker agent sessions for one free-form task: `fleet <task>` makes the invoking session the operator. It launches workers in tmux via the launcher matching its own engine, watches them with the parse-free watcher, steers and recovers them with the recipes below, and scales ceremony to task weight — from one-liner dispatch tasks up to full goal engagements with a local engagement record. No external service is required: by default the watcher calls nothing outside the host. Use whenever a task should be split across parallel worker sessions."
---

# Fleet operator playbook

This skill is a playbook, not a dispatcher: it never writes `@fleet_*`
tmux options and never sends an exact-assignment carrier. Workers are
launched as standalone sessions; the operator records who owns which lane
in the engagement record, and that record is the authority for the
engagement.

## Coordinator and orchestrator

Two roles, kept apart so a demo or a review can point at each one:

- **Coordinator** — the invoking (operator) session. It interprets the
  task, splits it into single-owner lanes, decides the stop condition,
  steers workers, answers or relays human decisions, and winds the
  engagement down. Only the coordinator judges what a worker produced.
- **Orchestrator** — the machinery around the coordinator: the tmux
  engagement session holding one window per worker, the watcher that
  wakes the coordinator when a worker goes stale or an optional wake
  source reports a new ID, and the tick schedule that keeps supervision
  alive past the end of any one operator turn. It never judges content
  and never inspects pane or transcript content; it only reports that
  something needs a look.

The entire feature is `fleet <task>`. There are no verbs: you — the
invoking session — are the operator, `<task>` is free-form, and you
interpret it. Everything durable lives in the per-engagement state dir
and, for goal engagements, the engagement record inside it
(`engagement.md`; `stop-condition` and `tick-schedule` live in the state
dir for every engagement); the only other machinery is the step-6 tick
schedule. The state dir defaults to `/tmp/fleet-<engagement>` on the host
running the fleet.

## Scale ceremony to task weight

- **Dispatch task** (e.g. "run the review with 4 workers"): workers +
  watcher + babysitting. No engagement record, no lanes table beyond the
  lane list in the state dir. The step-1 stop condition and step-6 tick
  schedule live in the watcher state dir.
- **Goal engagement** (a multi-lane objective with human decisions): the
  full loop below — engagement record, lane table, ticks, wind-down.

Worker count is whatever the task says. One caution you state before
launch: a host-global capacity limit or a shared rate limit can make N
workers no faster than one — warn, then obey the user's word.

## Launcher table (pick by YOUR engine — no detection code)

Launch the same engine you are running as. Each worker gets its own
worktree (or the engine's equivalent isolated working copy) and the lane
prompt as its first input. If your engine's CLI spells worktree or
session launch differently, use its documented equivalent; the topology
in the next paragraph is the requirement, not the exact flags.

| Operator session engine | Worker launcher | Notes |
|---|---|---|
| Muse Code | `muse --yolo -w create '<prompt>'` | prompt rides argv; `-w create` = fresh worktree per worker; interactive TUI |
| Claude Code | `claude` in a fresh worktree, lane prompt as the first input | isolated worktree per worker; interactive TUI |
| Codex | `codex` in a fresh worktree, lane prompt as the first input | isolated worktree per worker; interactive TUI |

Running on a Linux host: `reference/linux-host.md` covers what a Linux
worker may take and the host prerequisites.

Running in Herdr: identify the stack first. In an hc terminal
(`HC_TERMINAL_ID` set), `reference/herdr-core-backend.md` maps this
playbook to herdr-core terminals and detached fleet lanes. In an old
Herdr pane (`HERDR_ENV=1` only), `reference/herdr-backend.md` maps this
playbook to Herdr workspaces and the `fleet-herdr` verbs. Either way,
the playbook still owns ceremony, stop conditions, recovery judgment,
and wind-down gates.

Every worker runs inside its own tmux window of the engagement session —
NEVER exec a TUI outside tmux (a killed exec strands its worktree). A
fleet worker IS a standalone main session in its own tmux window — never
a spawned subagent (a subagent dies with the operator, has no transcript
for the watcher/recipes, and breaks the resume contract). The initial
prompt can ride argv where the engine supports it; otherwise launch the
TUI in the window first, then deliver the prompt with the send recipe
below.

Pin the shared default tmux socket before any engagement command. Some
launchers run the operator in a private tmux namespace that is torn down
when the launcher exits, so an engagement built with bare `tmux` there is
invisible to peers and to a resume operator. Pin `TMUX_TMPDIR` to the
default root (`/tmp` → socket `/tmp/tmux-$(id -u)/default`) so the
command targets the durable socket.

**Prefix EVERY engagement `tmux` command with `TMUX_TMPDIR=/tmp env -u TMUX`** —
including `new-session`, `new-window`, `send-keys`, `capture-pane`,
`kill-session`, and `tmux ls`. Two reasons: each shell tool call is a
fresh shell, so an `export` does not carry across calls; and tmux picks
its server `-S`/`-L` > `$TMUX` > `TMUX_TMPDIR`, so a shell that already
carries `$TMUX` (a bare engine in a pane on a non-default server) would
ignore `TMUX_TMPDIR` — `env -u TMUX` drops it so the socket root wins.
The recipes below show the prefix; apply it to every `tmux` invocation
you run, not only the fenced ones.

```sh
TMUX_TMPDIR=/tmp env -u TMUX tmux new-session -d -s <engagement> -n operator
TMUX_TMPDIR=/tmp env -u TMUX tmux new-window -t '=<engagement>' -n w1 'muse --yolo -w create <prompt>'
```

Run the engagement commands from the repo root (`git rev-parse
--show-toplevel`): new windows inherit the invoking shell's cwd, so only
the repo root puts the launcher path and the worker's worktree in the
right place.

## Goal-engagement loop

1. **Intake.** Restate the task, split it into single-owner lanes, decide
   worker count and engine rows. Declare the engagement's stop/wind-down
   condition up front — a time box, a per-worker finish count, or an
   explicit handoff — for goal engagements AND dispatch tasks alike.
   Record it in the engagement record (goal engagements) or the watcher
   state dir `stop-condition` file (dispatch tasks — `mkdir -p` the
   state dir when you record it; the watcher's own `mkdir -p` runs in
   step 5).
2. **Engagement record.** Create one record in the state dir
   (`engagement.md`): task, lane table (lane → builder → reviewer →
   worktree/branch → transcript path), decisions log. Every durable
   fact goes here —
   see the resume contract below. If the project uses an issue tracker
   and the human asks for a mirror there, copy the record there too;
   the local record stays the authority and the copy is never required.
3. **Human channel.** Agree one channel with the human (a direct report,
   a message thread, an issue comment — the skill does not pick one) and
   note it in the record. Humans answer decisions there; you never block
   a lane on an answer a worker can prep around.
4. **Workers.** One tmux window per worker, launcher per the table.
   Worker prompts state: the single lane, hard limits, report
   conventions (`STATUS:` progress, `BLOCKED(HUMAN):` one-line
   questions), and the stop condition. Record each worker's transcript
   path: take it from the engine's session record where the engine
   keeps one; where it keeps none, start the worker with its output
   recorded to a per-worker log file in the state dir and treat that
   file as the transcript for staleness and resume. Stop conditions are
   end-to-end by default: the lane is done only at a verified end state
   declared at intake (for a code lane, typically its tests pass and its
   change is merged — verify a merge by merge-commit ancestry, never by
   a merged flag alone). Open, running, and awaiting states are active
   babysitting states, not terminal ones: the worker keeps polling and
   reacting (failures, replies, readiness) and reports `STATUS:` per
   round. The only stop-before-done is a critical decision a human must
   make: the worker reports `BLOCKED(HUMAN)` and you surface that exact
   question to the human (human channel / engagement record / direct
   report) instead of letting the lane idle silently. A lane stalled
   awaiting another person or system gets escalated by you through the
   human channel — never left idle with no owner. A worker's time-box
   share of the stop condition never authorizes abandoning unfinished
   work — an unfinished lane is babysat until it finishes or a human
   stops the lane.
5. **Watcher.** Start it detached (requires GNU `timeout`; macOS:
   `brew install coreutils`):

   ```sh
   nohup .agents/skills/fleet/watcher.sh /tmp/fleet-<engagement> \
     <transcript>... >/tmp/fleet-<engagement>.watcher.log 2>&1 &
   ```

   Log to a file, not `/dev/null`: both the launch guard's refusal (a
   transcript argument that is not an existing regular file — the zsh
   collapsed-list case) and a missing-`timeout` dependency exit print
   to stderr and land in `/tmp/fleet-<engagement>.watcher.log`. The
   guard exits before `pid`/`heartbeat` are written, but an add-worker
   relaunch that is refused leaves the PREVIOUS watcher's `pid` file
   in place — so treat the launch as failed if `pid` is missing OR its
   value did not change from before the launch, and read the log for
   the named bad argument.

   Shell note: your shell tool may be zsh, which does NOT word-split an
   unquoted `$var`. If you pass the transcript list (or the
   coverage-check list below) through a variable, run that launch/check
   under `bash` (`bash -c '...'`) — under zsh all paths arrive as ONE
   argument, which silently loses per-worker staleness coverage and
   false-passes the coverage check. The watcher now refuses to start
   when any transcript argument is not an existing regular file (loud
   launch failure), which catches the collapsed mega-path, but running
   under `bash` is what makes the split happen in the first place.

   Confirm the watcher is alive before ticking —
   `kill -0 "$(cat /tmp/fleet-<engagement>/pid)"` — treating a missing or
   unchanged `pid` (an add-worker relaunch leaves the previous watcher's
   `pid` behind) as a failed launch and reading the log above for why.

   The watcher records ITS OWN pid at `/tmp/fleet-<engagement>/pid` —
   use that as the kill handle, never the launcher's `$!` (under job
   control it can be an intermediate pid). The pid file is a hint, not
   proof: always kill with the validated kill below, which refuses a
   pid that is not this engagement's watcher (a stale file, a recycled
   pid, or a sibling engagement's watcher must never be killed). It
   appends wake reasons to `wake.txt` (a worker transcript stale ≥10
   min; a new wake-source ID; `wake-source-failing` when an optional
   wake source keeps failing — every failure is also timestamped in
   `wake-source.err`, so a broken wake source never reads as quiet) and
   refreshes `heartbeat`. It parses nothing — pane and transcript
   content are read only by you, at ticks. With no `FLEET_LIST_IDS_CMD`
   override the watcher runs no wake-source check at all. The state dir
   also holds the engagement's `stop-condition` file (step 1) and the
   `tick-schedule` handle (step 6), so a fresh reader finds both
   without an engagement record.

   ```sh
   S=/tmp/fleet-<engagement>  # the watcher state dir from step 5
   pid=$(cat "$S/pid")
   cmd=$(ps -ww -p "$pid" -o command= 2>/dev/null || true)
   case "$cmd" in
   *"/fleet/watcher.sh $S"|*"/fleet/watcher.sh $S "*) kill "$pid" ;;
   *) echo "stale pid handle: $pid is not this engagement's watcher (ps: ${cmd:-no such process}); not killing" >&2 ;;
   esac
   ```
6. **Tick.** Supervision must survive the end of any turn. While an
   operator session lives, never leave workers running with
   neither an in-turn reader nor an armed schedule. Exiting with workers running
   is permitted; before you leave, run the `tick-schedule` cancel
   command; the declared stop condition bounds them and the next
   session adopts them via the resume reads. Arm a recurring scheduled
   self-check at ~10 min (the watcher stale threshold; longer only when
   the step-1 stop condition explicitly allows it) on your harness's
   scheduler (cron) when you start the watcher (step 5), before the
   first worker can stall: each check re-enters, performs one full
   tick, and re-arms itself until the stop/wind-down condition
   (step 1), then winds down (step 8) and cancels the schedule. Record
   the schedule handle in the state dir `tick-schedule` file (always;
   goal engagements also copy line 1 into the engagement record) as a
   one-line cancel command (line 1 = the cancel command — run it at
   leave, wind-down, resume; line 2, host one-shot only = the arm-time
   pane record — data, never a command; never `sh` the file): the
   harness cron delete command or `systemctl --user stop <unit>` (a
   host one-shot needs no standing entry; its cancel is a no-op after
   it fires). An operator engine with no harness scheduler meets the
   invariant through a host one-shot instead — on Linux hosts only
   (see `reference/linux-host.md` for the host prerequisites) — and
   only when the operator itself runs under tmux: `systemd-run --user
   --on-active=10m` firing `tmux send-keys -t <your pane>` a tick
   prompt, re-issued at the end of every delivered tick — record your
   pane id, pid, and current command (`tmux -S "$SOCK" display -p -t
   "$TMUX_PANE" '#{pane_id} #{pane_pid} #{pane_current_command}'`) in
   `tick-schedule` at arm time (second line, after the cancel command);
   the bootstrap `operator` window is a bare shell, never the target.
   An operator with neither a harness scheduler nor a systemd host
   meets the invariant only through in-turn presence while it lives
   (the sleep-slice loop below, until wind-down or handoff) — workers
   that outlive it are bounded by the step-1 stop condition alone.
   Resolve `SOCK="${TMUX:-}"; SOCK="${SOCK%%,*}"` first and prefix
   `-S "$SOCK"` to every arm-time tmux call below. Arm a host one-shot
   only when `[ -n "${TMUX_PANE:-}" ]` and that SOCK is non-empty —
   otherwise do not dispatch, do not arm: stop before step 4 launches
   a worker and relaunch your engine in a NEW window of the
   engagement session (`tmux -S "$SOCK" new-window -t
   '=<engagement>' -n op ...`), never the bare `operator` shell, and
   arm from that pane. To stop mid-engagement follow the step-6 leave
   rule: run the `tick-schedule` cancel, then quit your engine so the
   pane drops to a shell and any armed fire no-ops — never
   `kill-session`. The fire payload re-reads `#{pane_pid}` and
   `#{pane_current_command}` for the recorded pane and types only
   when both match the arm-time values — using a reduced steering send
   sequence (`C-u`, `send-keys -l '<tick prompt>'`, ~1s wait, then
   `Enter` as a separate call; no `BSpace` sweep or capture-pane
   verify — the transcript-grep receipt is the check: a swallowed
   Enter ends the chain, while residue `C-u` leaves is delivered as a
   prefix to the intact tick text (accepted cost: the fire's `C-u`
   clips a one-line draft — including a human's direct steer — and
   submits a multi-line one fused with the tick text; in a goal
   engagement humans answer via the human channel, so a parked pane
   draft is off-path there; in a dispatch task (no human channel —
   the pane is the human's own channel), a draft left unsent across a
   ~10 min fire is clipped or fused — accepted and disclosed here
   rather than guarded)) — otherwise exits without typing. Bake the
   pane id, the arm-time pid/command, and that SOCK into the unit
   (`--setenv=PANE=... --setenv=ARMED="..." --setenv=SOCK=...`); a
   systemd unit inherits none of the arming shell's tmux env, so
   baking the resolved socket is what keeps the fire on the
   operator's server. The committed payload:

   ```sh
   PANE="${PANE:?}"      # arming pane id, baked in at arm time
   ARMED="${ARMED:?}"    # "pid command" recorded at arm time
   SOCK="${SOCK:?}"      # tmux socket, resolved at arm time — never empty
   T() { tmux -S "$SOCK" "$@"; }
   LIVE="$(T display -p -t "$PANE" '#{pane_pid} #{pane_current_command}')"
   [ "$LIVE" = "$ARMED" ] || exit 0   # stale fire: no-op
   T send-keys -t "$PANE" C-u
   T send-keys -t "$PANE" -l 'tick: scheduled self-check, run one full tick'
   sleep 1
   T send-keys -t "$PANE" Enter
   ```

   (`one-shot-smoke.sh` extracts this fence and runs it under `bash`
   with a stub `tmux`: mismatch → no `send-keys`; match → the sequence
   above.) No standing `crontab` line: it bounds the stale fire to one
   guarded fire — a no-op when the pane is back at a shell or runs a
   different command; a same-engine relaunch in the same pane within
   the window still receives it, and the fresh session's
   `tick-schedule` cancel closes that window. Each tick greps the
   transcript for the previous fire's turn as the delivery receipt
   (the steering rule: no transcript grep, no delivery) and only
   re-arms on receipt. A missed delivery ends the chain (a schedule
   lapse; see the resume contract); workers are then bounded only by
   the stop condition until the next session adopts them.
   Bound in-turn presence to launch plus the first few ticks (~1 hour
   max): sleep in short slices and tick EARLY when a wake lands, full
   tick at ~10 min either way:

   ```sh
   S=/tmp/fleet-<engagement>  # the watcher state dir from step 5
   : >>"$S/wake.txt"; n=$(wc -l <"$S/wake.txt"); for i in $(seq 20); do
     sleep 30; [ "$(wc -l <"$S/wake.txt")" -gt "$n" ] && break
   done  # then tick
   ```

   Each tick, in-turn or scheduled: read `wake.txt` tail, read new
   human-channel messages, `capture-pane` each worker, read stalled
   workers' transcripts. Steer, relay human answers, refill finished
   lanes, update the engagement record (or the state dir for dispatch
   tasks). A stale wake means idle-blocked, complete, stuck dialog,
   or dead — read the transcript before touching the pane. A scheduled
   tick that finds the stop/wind-down condition met winds down
   instead of re-arming.
7. **Pressure cleanup after worker all-clear.** Only after the current
   tick's worker handling reaches all-clear (no unresolved worker
   action or human escalation), check free space on the filesystem
   holding the repo. If free space is low enough to risk the run,
   pause and ask the human before deleting anything: there is no
   standing approval to delete here. A pending human-only question
   pauses here; never use cleanup to bypass it.
8. **Wind-down.** Verify each lane finished at its declared end state
   and its claim in the engagement record is released, post the
   closing note to the engagement record, cancel the tick schedule
   (step 6) by running its one-line cancel command, kill the watcher
   with the step-5 validated kill, tear down the engagement session
   with `TMUX_TMPDIR=/tmp env -u TMUX tmux kill-session -t '=<engagement>'`
   (the prefix targets the durable socket, per the launcher-section
   rule; workers are windows of that session — there are no
   per-worker session names to kill), and apply the
   worktree-cleanup gates before removing anything: a worktree or branch is removed
   only after its lane is verified done, its change is preserved in
   the repo, and the human has agreed to the removal. If the
   validated kill refuses on a live watcher, resolve it by hand
   first — a live watcher outlives the session teardown and keeps
   waking with no reader.

## Adding a worker mid-engagement

The watcher's transcript list is fixed at launch — a worker added later
is unwatched until the watcher relaunches with the extended list:

1. Split the lane and update the engagement-record lane table FIRST
   (new lane → worker → worktree/branch → transcript path, once
   known).
2. Launch the window per the launcher table; resolve the new worker's
   transcript path (same rule as step 4) and record it in the lane
   table.
3. Stop the watcher with the step-5 validated kill — never a raw pid.
   Relaunch only once `ps -p "$(cat "$S/pid")"` shows that pid is
   gone; a refusal on a live `watcher.sh` for this dir means the
   launch form differs — resolve that pid by hand first, never
   relaunch over it (an over-launch orphans the old watcher with the
   stale list and no kill handle; see Resume contract).
4. Relaunch into the SAME state dir with the full list: every
   still-live worker's transcript kept, dead workers' dropped, the
   new one appended. Keep the dir (the `seen` set and stale marks
   carry over); then confirm `heartbeat` refreshes under the new pid.
5. Run the coverage check — every live worker window's transcript
   must appear in the running watcher's tracked paths:

   ```sh
   # Run under bash if you substitute a variable for the `for t in ...`
   # transcript list below: zsh keeps it one mega-token, which matches
   # the space-joined argv and false-passes this check (see the step-5
   # shell note). ($tracked is always quoted, so it is safe.)
   S=/tmp/fleet-<engagement>  # the watcher state dir from step 5
   tracked=$(ps -ww -p "$(cat "$S/pid")" -o command=)
   for t in <transcript-1> <transcript-2> ...; do  # the full live set, from the lane table
     # Whole-token match (the padded spaces): a substring test would call
     # .../w1.json watched when only .../w1.jsonl is tracked.
     case " $tracked " in *" $t "*) ;; *) echo "UNWATCHED: $t" ;; esac
   done
   ```

   A dropped path silently loses staleness coverage for that worker —
   fix the argv and relaunch before the next tick. Re-run this check
   at any tick where the worker set changed.

## Reviewer workflow (normal agent review)

Every lane that produces a change also gets a reviewer: a normal agent
review by a separate worker session. The reviewer reads the change,
runs the checks, and writes a verdict. No separate review tool is
involved and no review service is required.

- **Separate session.** A reviewer is a standalone worker under the
  same topology rules: its own tmux window or Herdr workspace, its own
  worktree, its own transcript or agent read. A reviewer is never a
  subagent of the builder, and never edits the builder's worktree. It
  reads the builder's branch or diff from its own checkout.
- **Review inputs.** The lane brief and acceptance criteria, the diff
  against the base, and the test and parity results. The reviewer
  re-runs the lane's tests and parity check in its own worktree and
  treats a check it did not run as unproven.
- **Verdict.** Approve, or request changes, with one finding per item:
  file and line, what is wrong, the evidence (a failing test, a parity
  mismatch, or a quoted requirement), and what would close it.
  Findings without evidence go back to the reviewer, not to the
  builder.
- **Loop.** The builder addresses requested changes; the reviewer
  re-reviews only the changed files and the new evidence, then
  approves or requests changes again. After two request-changes
  rounds without agreement, the reviewer escalates to the operator,
  who decides or asks the human — the loop does not run unbounded.
- **No merge by the reviewer.** A reviewer never merges, and a
  reviewer's approval is not a merge decision. Merging stays with the
  operator and the human under the merge policy recorded at intake.
- **Counting.** A reviewer counts toward the worker count and the slot
  budget for the engagement, exactly like a builder. A lane with no
  reviewer is recorded as unreviewed in the engagement record; it is
  never silently treated as reviewed.

## Steering a worker TUI (exact send sequence, every time)

Each line carries the `TMUX_TMPDIR=/tmp env -u TMUX` socket prefix from
the launcher-section rule (each shell tool call is a fresh shell).

1. Clear composer residue: `TMUX_TMPDIR=/tmp env -u TMUX tmux send-keys -t '=<s>:<w>' C-u`,
   then ~25× `BSpace` (leftover text corrupts the message).
2. `TMUX_TMPDIR=/tmp env -u TMUX tmux send-keys -t '=<s>:<w>' -l '<text>'` — literal flag,
   no key-name parsing.
3. Sleep ~1s.
4. `TMUX_TMPDIR=/tmp env -u TMUX tmux send-keys -t '=<s>:<w>' Enter` as a SEPARATE call —
   packing text+Enter into one call gets the Enter eaten by the slash popup or
   inserted as a newline.
5. Verify with `TMUX_TMPDIR=/tmp env -u TMUX tmux capture-pane` that the text left the
   composer; the AUTHORITATIVE delivery receipt is grepping the worker's
   transcript for the sent text — a TUI that crashed to a shell still "looks"
   delivered.

Target hygiene: tmux `-t` is PREFIX-matched — after a session dies, `-t X`
can silently hit `X-heartbeat`. Scripts use exact forms: `has-session -t
'=X'` for sessions, `-t '=X:<window>'` for send-keys/capture-pane.

## Recovery recipes (judgment, not machinery)

- **Stale + transcript ends in a question** → answer it (or relay to the
  human) with the send sequence.
- **Frozen dialog / raw paste mode** (>4KB send: Enter inserts newlines,
  `C-u` won't clear) → ONE `Esc` exits paste mode, then a single Enter
  submits.
- **Connection reset at the prompt** → it never self-recovers: end the
  worker window, relaunch its engine with the session-resume flag in
  the same worktree.
- **Dead pane / crashed TUI** (status line is a shell prompt, not TUI
  chrome) → relaunch with resume; the worktree and transcript survive.
- **Worker went silent but its lane moved** → another driver may own
  the lane now; read the lane's record state before steering, never
  race it.
- Infra weather (queue churn, network or API storms): diagnose from
  the worker's transcript and the environment; fix the environment,
  then resume — don't restart lanes to "clear" infra errors.

## Resume contract (operator death is expected)

While the operator session lives, the armed schedule (step 6) is what
summons the next reader: every scheduled check adopts the engagement
via the reads below and re-arms until the stop/wind-down condition.
The engagement record is the durable state; tmux is the live registry.
A fresh operator session resumes by reading (in order): the engagement
record (lane table + decisions), `TMUX_TMPDIR=/tmp env -u TMUX tmux ls`
+ per-window capture-pane (same socket prefix as the
launcher-section rule), the watcher state dir (`wake.txt`,
`heartbeat`, `stop-condition`, `tick-schedule`, `wake-source.err` —
the quiet-vs-broken-wake-source signal), and each lane's record state.
Then it adopts the engagement: read `tick-schedule` and run its line-1
cancel command before arming your own (the file itself is data — never
execute it; a host entry outlives its session, a harness entry is
already dead and its cancel is a no-op). If the heartbeat is stale,
first run the step-5 validated kill (a stale heartbeat can still be a
live but stuck watcher — relaunching over it races the state dir and
orphans it without a kill handle). Relaunch only once `ps -p` shows
that pid is gone: a refusal on a live `watcher.sh` for this dir means
the launch form differs — resolve that pid by hand, never relaunch
over it. Then relaunch the watcher, and confirm the NEW watcher is
alive per step 5 before ticking; re-steer stalled workers, arm a new
schedule (overwriting `tick-schedule`), and continue ticks. Anything
not written to the engagement record is presumed lost with the
previous operator. If the schedule lapses (expiry, host reboot, manual
cancel, operator session exit, or an undelivered host one-shot fire
(delivery receipt is missing; nothing re-arms)), workers run
unsupervised until a human re-arms it with the same reads; the lapse
is bounded by the step-1 stop condition, after which workers wind
down on their own share of it.
