//! Hostile depth and length on a 256 KiB stack, every truncation and 10k mutations of the
//! corpus, and the deepest trees the derived impls are tested to survive.
use crate::common::{corpus, mutate};
use fastrand::Rng;
use placemat_core::kdl::{ReadErrorKind, ReadLimits, Value, parse};
use std::thread;

const SMALL_STACK: usize = 256 * 1024;
// Planning probe, dev profile, 256 KiB: Clone and Debug pass 200 and abort at 300; Drop passes 1,000.
const CLONE_DEPTH: u16 = 150;
const DROP_DEPTH: u16 = 1_000;

// A stack overflow aborts the whole binary, so each case runs on its own small-stack thread.
fn on_small_stack(case: impl FnOnce() + Send + 'static) {
    thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(case)
        .unwrap()
        .join()
        .unwrap();
}

fn nested(depth: usize) -> String {
    format!("{}{}", "a {".repeat(depth), "}".repeat(depth))
}

#[test]
fn hundred_thousand_nested_blocks_are_too_deep() {
    on_small_stack(|| {
        let Err(e) = parse(&nested(100_000), &ReadLimits::default()) else {
            panic!("100k nesting parsed")
        };
        // The 65th `{` is the first past the default depth of 64.
        assert_eq!((e.kind, e.offset), (ReadErrorKind::TooDeep, 3 * 65 - 1));
    });
}

#[test]
fn long_comments_strings_and_chains_return() {
    on_small_stack(|| {
        let d = ReadLimits::default();
        assert_eq!(
            parse(&format!("a /*{}*/", "*".repeat(1 << 20)), &d)
                .unwrap()
                .len(),
            1
        );
        assert!(
            parse(&format!("/{}", "*".repeat(1 << 20)), &d).is_err(),
            "unclosed comment"
        );
        assert_eq!(
            parse(
                &format!("{}{}a", "/* ".repeat(1_000), "*/ ".repeat(1_000)),
                &d
            )
            .unwrap()
            .len(),
            1
        );
        let _ = parse(&format!("{}a", "/- ".repeat(10_000)), &d);
        assert_eq!(
            parse(&format!("{}b", "/-a\n".repeat(10_000)), &d)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            parse(&format!("a \"{}\"", "x".repeat(1 << 20)), &d).unwrap()[0]
                .entries
                .len(),
            1
        );
    });
}

#[test]
fn closing_braces_inside_raw_and_multi_line_strings_stay_text() {
    on_small_stack(|| {
        let nodes = parse(
            "a {\n b #\"}\"# \"\"\"\n }\n \"\"\"\n}\nc",
            &ReadLimits::default(),
        )
        .unwrap();
        assert_eq!(nodes.len(), 2);
        let values: Vec<&Value> = nodes[0].children.as_ref().unwrap()[0]
            .entries
            .iter()
            .map(|e| &e.value)
            .collect();
        assert_eq!(
            values,
            [&Value::String("}".into()), &Value::String("}".into())]
        );
    });
}

#[test]
fn trees_at_the_tested_depths_clone_compare_format_and_drop() {
    on_small_stack(|| {
        let nodes = parse(
            &nested(usize::from(CLONE_DEPTH)),
            &ReadLimits::default().with_max_depth(CLONE_DEPTH),
        )
        .unwrap();
        assert!(nodes.clone() == nodes, "clone equals original");
        assert!(!format!("{nodes:?}").is_empty(), "debug output");
    });
    on_small_stack(|| {
        drop(
            parse(
                &nested(usize::from(DROP_DEPTH)),
                &ReadLimits::default().with_max_depth(DROP_DEPTH),
            )
            .unwrap(),
        );
    });
}

#[test]
fn every_prefix_of_every_corpus_input_returns() {
    for (_, text) in corpus::inputs() {
        for end in (0..=text.len()).filter(|&i| text.is_char_boundary(i)) {
            let _ = parse(&text[..end], &ReadLimits::default());
        }
    }
}

#[test]
fn child_parses_ten_thousand_mutations() {
    let inputs = corpus::inputs();
    for seed in 0..10_000u64 {
        let mut rng = Rng::with_seed(seed);
        let (_, source) = &inputs[rng.usize(..inputs.len())];
        let _ = parse(&mutate::mutate(&mut rng, source), &ReadLimits::default());
    }
}

// A panic or abort fails the child; anything it writes to stderr fails the parent. A filter
// matching no test also exits 0, so the child must report exactly one test passed.
#[test]
fn mutations_neither_panic_abort_nor_print() {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "robustness::child_parses_ten_thousand_mutations",
            "--test-threads=1",
            "--nocapture",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "child failed: {:?}\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stderr.is_empty(),
        "the reader wrote to stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("test result: ok. 1 passed;"),
        "the child ran no mutation test: {stdout}"
    );
}
