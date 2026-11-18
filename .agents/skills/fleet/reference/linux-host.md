# Running a fleet on a Linux host

A Linux fleet is an ordinary independent fleet: fleet state is tmux-local
per machine, and the engagement record in the watcher state dir is the
only durable state — so a Linux fleet coexists with a fleet on another
host with no coordination setup. The operator, workers, watcher, and
tick schedule all run on the same host; nothing in this skill coordinates
across hosts.

## What a Linux worker takes

The same lanes as any other host, subject to the host's tooling: work
that needs macOS- or Windows-only tools cannot run here. A lane report
from a Linux worker names what was NOT run and why (for example,
platform-only end-to-end checks), and the operator treats a Linux-only
green run as evidence, not as proof for another platform.

## Host prerequisites

- tmux, git, GNU `timeout` (coreutils).
- The worker engine's CLI, authenticated the way that engine requires,
  with a working worktree launch (see the launcher table in
  `../SKILL.md`).
- The repo checked out at the root the engagement runs from.

## Known asymmetries vs a Mac host

- GUI end-to-end checks that need a Mac desktop cannot run: never
  dispatch desktop-surface work to a Linux worker.
- Platform signing, sandbox, and keychain failures cannot be reproduced
  here (nor suffered): a lane whose end state needs them must run on a
  host that has them.
- The tmux socket pinning in `../SKILL.md` applies unchanged; the
  default socket root is `/tmp` on Linux as on macOS.
