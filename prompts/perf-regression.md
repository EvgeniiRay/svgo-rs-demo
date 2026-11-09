# Find and fix a performance regression in svgo-rs

You run unattended in GitHub Actions on a pull request. Nobody will answer
questions. The Rust crates and the JS SVGO reference are already installed.

The benchmark gate found that this pull request makes the optimizer slower than
main. The numbers, the pull request's diff against main and a profile of both
builds follow this prompt.

1. Find the change that causes the slowdown. Use the diff and the two profiles;
   run `cargo build --release --example bench_corpus` and
   `./target/release/examples/bench_corpus corpus 5` to measure, and profile
   again if you need to (`scripts/ci/profile.sh <binary>`).
2. Fix it without losing what the pull request wanted. If the change adds a
   check or a feature, keep it where it is cheap (for example in debug builds and
   tests) or make it cheaper. Do not just revert the pull request.
3. Show the fix works: the benchmark is back within 5% of main, `cargo test
   --release --locked` passes for the root crate and the six group crates, and
   the corpus parity check from the README stays at 2478/2478.
4. Do not commit or push; the workflow does that.

Your final answer must follow the JSON schema you were given.
