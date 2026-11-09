//! Corpus benchmark: loads every SVG in a folder into memory once, then
//! optimizes the whole set several times and prints one JSON line.
//! `cargo run --release --example bench_corpus -- corpus 7`
//! File I/O stays out of the timed loop, so the number measures the optimizer only.

use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| "corpus".into());
    let runs: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(7);
    let mut inputs: Vec<String> = std::fs::read_dir(&dir)
        .expect("corpus folder")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "svg"))
        .map(|p| std::fs::read_to_string(p).expect("svg"))
        .collect();
    inputs.sort();
    // One warm-up pass, not timed.
    for s in &inputs {
        let _ = svgo_rs::optimize_preset(s);
    }
    let mut ms: Vec<f64> = (0..runs)
        .map(|_| {
            let t = Instant::now();
            for s in &inputs {
                std::hint::black_box(svgo_rs::optimize_preset(std::hint::black_box(s)).ok());
            }
            t.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = ms[ms.len() / 2];
    println!(
        "{{\"icons\":{},\"runs\":{},\"median_ms\":{:.1},\"min_ms\":{:.1},\"max_ms\":{:.1},\"icons_per_s\":{:.0}}}",
        inputs.len(), runs, median, ms[0], ms[ms.len() - 1], inputs.len() as f64 / (median / 1000.0)
    );
}
