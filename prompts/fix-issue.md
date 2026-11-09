# Fix a reported bug in svgo-rs

You run unattended in GitHub Actions. Nobody will answer questions, and the job
has no network beyond the package proxy: the Rust dependencies and the JS SVGO
reference are already installed.

svgo-rs is a Rust port of SVGO's preset-default. The JavaScript reference is
`reference/svgo` (SVGO 4.1.0). Its output is the ground truth: svgo-rs must
produce byte-identical output.

The bug report follows this prompt, after the line `--- issue ---`.

1. Reproduce it first. Run the input through `reference/svgo` (node) and through
   `cargo run --release -- -` and compare. If you cannot reproduce it, stop and
   say so in the result.
2. Add a regression test that fails before your fix: a fixture-style test in the
   crate that owns the plugin, with the JS output as the expectation.
3. Fix the Rust code. Keep the change small and in the style of the crate.
4. Run `cargo test --release`, the group crate tests for the crate you changed
   (`cargo test --release -p <crate>`), and the corpus parity check from the
   README. All must pass.
5. Do not commit or push; the workflow does that.

Your final answer must follow the JSON schema you were given: what was wrong,
what you changed, which test you added and the test and parity results.
