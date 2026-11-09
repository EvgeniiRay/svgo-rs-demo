# Make svgo-rs faster

You run unattended in GitHub Actions. Nobody will answer questions. The Rust
crates and the JS SVGO reference are already installed.

The workload is the 2,478 Fluent UI icons in `corpus/`. A profile of the current
build follows this prompt.

1. Find the hottest path that you can make faster with one focused change. Read
   the profile, then the code. Measure with
   `cargo build --release --example bench_corpus` and
   `./target/release/examples/bench_corpus corpus 5`, and profile again with
   `scripts/ci/profile.sh <binary>` when it helps.
2. Make one change, small and in the style of the crate. Output must not change:
   the corpus parity check from the README must stay at 2478/2478 byte-identical,
   and `cargo test --release --locked` must pass for the root crate and the six
   group crates.
3. Measure before and after with the same command. Keep the change only if it is
   at least 5% faster.
4. Do not commit or push; the workflow does that.

Your final answer must follow the JSON schema you were given.
