# Baseline before the iteration (branch iterate, from main 6f9e889)

Measured on the demo laptop (Windows 11, 2026-11-09) unless noted.

| Check | Before |
| --- | --- |
| Byte parity vs JS SVGO, 2,478 corpus icons | 2,478/2,478 byte-identical |
| Tests (`cargo test --release -p svgo-rs -p svgo-g1 … -p svgo-g6`) | 88 pass, 0 fail |
| Edge cases (`node scripts/edge_cases.mjs`: every upstream plugin fixture input through the full default preset) | 318/373 identical, 55 differ (see edge-cases.txt) |
| Hyperfine, folder mode over the corpus (2026-11-07 take, this laptop) | JS SVGO 3.829 s, svgo-rs 2.993 s (1.28×); svgo-rs uses one thread |
