//! Failures the Azure Pipelines demo can switch on. Both tests pass unless their
//! environment switch is set, so a normal `cargo test` stays green.
//!
//! - `SVGO_DEMO_FLAKY=1`: `corpus_batch_finishes_in_time` races a timer against a
//!   batch run and fails about half the time, like a test that depends on machine load.
//! - `SVGO_DEMO_BREAK=1`: `preset_keeps_view_box` checks a behavior the port gets
//!   right, against a wrong expectation, so it fails every time.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use svgo_rs::optimize_preset;

const ICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><path d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Z" fill="#212121"/></svg>"##;

#[test]
fn corpus_batch_finishes_in_time() {
    let start = Instant::now();
    for _ in 0..200 {
        optimize_preset(ICON).expect("optimize");
    }
    let mut budget = Duration::from_secs(5);
    if std::env::var("SVGO_DEMO_FLAKY").as_deref() == Ok("1") {
        // Simulated load spike: half of the runs get a budget that is too small.
        // Clocks differ in resolution, so mix the microseconds with the process id.
        let micros = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_micros();
        if (micros ^ std::process::id()) % 2 == 0 {
            budget = Duration::from_nanos(1);
        }
    }
    let took = start.elapsed();
    assert!(took <= budget, "batch of 200 icons took {took:?}, budget {budget:?} (timed out waiting for the worker pool)");
}

#[test]
fn preset_keeps_view_box() {
    let out = optimize_preset(ICON).expect("optimize");
    let expected = if std::env::var("SVGO_DEMO_BREAK").as_deref() == Ok("1") {
        r#"viewBox="0 0 48 48""#
    } else {
        r#"viewBox="0 0 24 24""#
    };
    assert!(out.contains(expected), "expected {expected} in {out}");
}
