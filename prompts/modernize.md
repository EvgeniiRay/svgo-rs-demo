# Modernize the workspace

You run unattended in Azure Pipelines on Windows. Nobody will answer questions.
Rust, Node and the JS SVGO reference are installed.

Move this Rust workspace (the root crate `svgo-rs` and the six group crates in
`crates/`) to current Rust, without changing what it does:

1. Edition 2024 in every crate. Set `edition`, `version`, `license` and
   `rust-version` once in `[workspace.package]` and inherit them.
2. Replace the `unsafe` block in `crates/g5` with safe code.
3. Use modern idioms where they make the code clearer: let-else,
   `is_some_and`/`is_none_or`, `LazyLock` instead of hand-rolled lazy statics,
   iterator methods instead of index loops, std APIs instead of hand-written
   helpers.
4. Make `cargo clippy --workspace --all-targets -- -D warnings` pass.

Work as a team. Start a workflow: first plan the shared changes (the workspace
manifest and anything every crate needs), then one agent per crate in
parallel, then an integration step that builds the whole workspace and fixes
what the crates' changes broke between them, then a critic that reads the whole
diff for behavior changes.

Prove it before you finish:

- `cargo test --release --locked` passes for the root crate and all six group
  crates.
- Corpus parity stays 2478/2478 (see the README).
- `./target/release/examples/bench_corpus corpus 5` is no slower than before
  (within 5%).

Do not commit or push; the pipeline does that.

Your final answer must follow the JSON schema you were given.
