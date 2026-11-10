# svgo-rs

A Rust port of [SVGO](https://github.com/svg/svgo) 4.1.0 `preset-default` (all 34 plugins), made with
[Muse Code](https://dev.meta.ai) for a demo: six agents ported the plugins in parallel git worktrees, a lead
agent integrated them, and a parity agent proved byte-identical output against JS SVGO on 2,478 icons.

The repository also holds the pipeline demos that run Muse Code headless with `muse exec`:

| Demo | File | What happens |
| --- | --- | --- |
| Issue to pull request | `.github/workflows/muse-fix.yml` | Label an issue `muse-fix`. Muse Code reproduces the bug against JS SVGO, adds a regression test, fixes it and opens a linked pull request. `docs/demo-issue.md` is a real bug to try it with. |
| Nightly upstream sync | `.github/workflows/nightly-sync.yml` | Every night, new upstream SVGO commits are ported in a fresh worktree, checked for parity and proposed as a pull request. `scripts/nightly/nightly-sync.ps1` is the Windows Task Scheduler version. |
| Windows pipeline | `azure-pipelines.yml` | A Windows build on Azure Pipelines. When tests fail, a read-only Muse Code run (`--disable-write --disable-shell`) reads the log and decides flaky or real; a flaky failure gets one rerun (set the run parameter `demoFailure` to `flaky` or `real` to try it). On pull requests a perf gate times the change against main on the agent; above +10% Muse Code fixes the slowdown, the pipeline checks tests, parity and speed again and pushes the fix. Run it by hand with `modernize` checked and a team of Muse Code agents moves the workspace to current Rust, checked by clippy, tests, parity and speed, then opens a pull request. |
| Benchmark gate (GitHub) | `.github/workflows/perf-guard.yml` | The same perf gate on a hosted runner, run by hand. |
| Optimization hunt | `.github/workflows/perf-hunt.yml` | Run it by hand. Muse Code profiles the corpus run, makes the hottest path faster with one change and opens a pull request with the benchmark table, after the workflow rechecks parity (2478/2478) and tests. |
| Security review | `.github/workflows/security-review.yml` | A read-only Muse Code run (`--disable-write --disable-shell`) reviews each pull request. A finding fails the check; a second run proves it with a failing test, fixes it and opens a pull request with the evidence. |
| Site audit | `.github/workflows/site-audit.yml` | Each pull request that changes the docs site (`site/`, published by `pages.yml`) runs Lighthouse on every page and checks every local link. Under the bar, Muse Code fixes what failed; the workflow audits again outside Muse Code, pushes the fix and posts the scores before and after. |

Each demo needs one secret, `MUSE_API_KEY`, from a Muse Code developer account: a GitHub Actions secret, or an
Azure Pipelines secret variable. The GitHub workflows pipe it to `muse exec --api-key-stdin`; the Azure pipeline
maps it to `META_API_KEY`, which Muse Code reads. Neither prints it.

The JavaScript reference is `reference/svgo/` (SVGO 4.1.0, MIT, see its LICENSE) and the test corpus is `corpus/`
(Fluent UI System Icons, MIT, see `corpus/LICENSE`).

## Usage

```sh
cargo build --release
./target/release/svgo-rs input.svg -o output.svg
./target/release/svgo-rs -f corpus -o out-dir   # batch mode
cat input.svg | ./target/release/svgo-rs -      # stdin -> stdout
```

Output matches JS SVGO `optimize()` with default settings: compact
serialization, no trailing newline.

## Layout

| Path | Contents |
| --- | --- |
| `src/lib.rs` | Preset pipeline (34 plugins, in order) + single-plugin dispatch |
| `src/main.rs` | CLI (file, stdin/stdout, `-f` folder batch mode) |
| `src/lead_plugins.rs` | Lead-ported `removeDeprecatedAttrs`, `removeUnknownsAndDefaults`, `sortAttrs`, `sortDefsChildren` |
| `src/lead_data.txt` | Element/attribute tables generated from JS `_collections.js` |
| `crates/g1` … `crates/g6` | The six agent groups (cleanup / styles / numeric+color / structure / path / misc) |
| `tests/preset_fixtures.rs` | All 283 upstream preset fixtures through the integrated dispatch |
| `scripts/` | Parity harness, JS corpus runner, lead-data generator |

Plugins run in preset order as parse → apply → serialize stages over
each group crate's DOM, with a final compact serialization. Shared
helpers the agents flagged (JS number rounding, color tables, CSS and
path-data parsing) were reconciled by the lead; in particular
`crates/g5/src/tools.rs` implements SVGO's `toFixed` helper with JS
`Math.round` semantics, which was the root cause of all initial
corpus mismatches.

## Verification (2026-11-07)

| Check | Result |
| --- | --- |
| Upstream fixtures, six group crates | 257/257 pass |
| Upstream fixtures, integrated preset (`cargo test`) | 283/283 pass |
| Byte parity vs JS SVGO, 2,478 corpus icons | **2,478/2,478 byte-identical** |
| Hyperfine, JS SVGO (Node, corpus batch) | 3.829 s ± 0.294 |
| Hyperfine, svgo-rs (release, corpus batch) | 2.993 s ± 0.355 (−21.8%, ~1.28×) |

Reproduce:

```sh
cargo test
cargo build --release
node scripts/run_js_corpus.mjs corpus parity-js
./target/release/svgo-rs -f corpus -o parity-rs
python scripts/compare_parity.py parity-js parity-rs corpus
```

## Known limitations

Parity is proven on this corpus and the upstream fixture suite, not
on arbitrary SVGs. Documented gaps inherited from the group agents:
the CSS engine is a fixture-faithful subset of css-tree/CSSO (no full
restructuring, specificity, or `var()` handling); the XML parsers are
hand-written subsets of SAX (no custom-entity expansion or strict
error positions); `RegexLite` covers only the RegExp subset used by
`removeComments` preserve patterns; JS `Number#toString` exponential
formatting for |x| ≥ 1e21 or < 1e-6 is approximated. Areas beyond
fixture coverage in those categories may diverge from JS SVGO.
