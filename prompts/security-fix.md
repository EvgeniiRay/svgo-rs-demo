# Fix a security problem found in review

You run unattended in GitHub Actions. Nobody will answer questions. The Rust
crates are already installed.

A read-only security review of this pull request found the problem described
after this prompt.

1. Prove it first: add a test that fails because of the problem. Never use a real
   secret; the test creates its own canary file in a temporary folder and checks
   that the canary text cannot leak.
2. Run the test and keep its failing output.
3. Fix the code so the feature keeps working for safe inputs, then run the test
   again and `cargo test --release --locked` for the root crate.
4. Do not commit or push; the workflow does that.

Your final answer must follow the JSON schema you were given.
