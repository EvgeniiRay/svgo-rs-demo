# Nightly upstream sync ({{DATE}})

You are the nightly sync job for svgo-rs, the Rust port of SVGO's preset-default. You run unattended in a fresh
worktree on branch sync/{{DATE}}. Nobody will answer questions.

Upstream SVGO (C:\demo\svgo, origin/main) has these commits since the last sync ({{LAST}}) that touch plugins/ or lib/:

{{COMMITS}}

For each commit, oldest first:
1. Read it with `git -C C:\demo\svgo show <sha>`.
2. Port the behavior change to the Rust crate. Performance-only commits need no port if the Rust code has no
   equivalent of the cost they remove; say so in the report.
3. Port any fixture tests the commit adds or changes.

Then move reference/svgo to the newest commit above, run `cargo test` and the repo's parity check over the corpus,
and fix any mismatch. Commit each upstream commit's port separately, with the upstream sha in the message, then write
`.sync/PR-{{DATE}}.md`: a pull request body that lists the upstream commits, what changed in the port, the parity result
and the test count. Update `.svgo-sync` to the newest upstream sha. Don't push; the job script does.
