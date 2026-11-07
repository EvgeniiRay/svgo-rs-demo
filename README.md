# svgo-rs

Rust port of SVGO 4.1.0 `preset-default` (all 34 plugins), built by
six parallel agents in separate git worktrees, then integrated by the
lead. The JavaScript reference is `reference/svgo/` (v4.1.0) and the
test corpus is `corpus/` (2,478 Fluent UI 24px regular icons).

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
