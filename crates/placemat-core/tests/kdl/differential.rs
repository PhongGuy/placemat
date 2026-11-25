//! Our reader and kdl 6.7.1 build the same tree from generated documents, plus a release timing.
use crate::common::{kdlgen, plain};
use fastrand::Rng;
use placemat_core::kdl::{ReadLimits, parse};
use std::{thread, time::Instant};

const SEEDS: u64 = 2_000;

#[test]
fn our_tree_matches_kdl_on_generated_documents() {
    // kdl recurses, so it runs on a 64 MiB stack; the generator stays far below its abort points.
    let failures = thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(|| {
            let mut failures = Vec::new();
            for seed in 0..SEEDS {
                let text = kdlgen::document(&mut Rng::with_seed(seed));
                let ours = parse(&text, &ReadLimits::default()).map(|n| plain::from_ours(&n));
                let theirs = text
                    .parse::<kdl::KdlDocument>()
                    .map(|d| plain::from_kdl(&d));
                match (&ours, &theirs) {
                    (Ok(a), Ok(b)) if a == b => {}
                    _ => failures.push(format!(
                        "seed {seed}\n{text}\nours: {ours:?}\nkdl:  {theirs:?}"
                    )),
                }
            }
            failures
        })
        .unwrap()
        .join()
        .unwrap();
    assert!(
        failures.is_empty(),
        "{} of {SEEDS} differ; first:\n{}",
        failures.len(),
        failures.first().map_or("", String::as_str)
    );
}

#[test]
#[ignore = "release-only timing: heavy cargo test --release -p placemat-core --test kdl differential::time -- --ignored --nocapture"]
fn time_a_one_mebibyte_document() {
    let mut rng = Rng::with_seed(1);
    let mut text = String::new();
    while text.len() < 1 << 20 {
        text.push_str(&kdlgen::document(&mut rng));
    }
    let start = Instant::now();
    let nodes = parse(&text, &ReadLimits::default()).unwrap();
    eprintln!(
        "{} bytes, {} top-level nodes, {:?}",
        text.len(),
        nodes.len(),
        start.elapsed()
    );
}
