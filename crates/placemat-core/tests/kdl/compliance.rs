//! The reader against kdl 6.7.1's compliance corpus, compared as normalised trees.
use crate::common::{corpus, plain};
use placemat_core::kdl::{ReadLimits, parse};
use std::fs;

#[test]
fn the_corpus_is_vendored_whole_with_its_licence() {
    let root = corpus::root();
    assert_eq!(
        corpus::kdl_files(&root.join("input")).len(),
        320,
        "input cases"
    );
    assert_eq!(
        corpus::kdl_files(&root.join("expected_kdl")).len(),
        232,
        "expected outputs"
    );
    assert!(
        fs::read_to_string(root.join("NOTICE"))
            .unwrap()
            .contains("Kat Marchán"),
        "NOTICE"
    );
    assert!(
        fs::read_to_string(root.join("LICENSE"))
            .unwrap()
            .contains("Apache License"),
        "LICENSE"
    );
    assert!(
        fs::read(root.join("input/crlf_between_nodes.kdl"))
            .unwrap()
            .contains(&b'\r'),
        "CRLF kept"
    );
}

#[test]
fn numbers_canonicalise_to_kdls_stored_form() {
    for (text, want) in [
        ("0xFF_ff", "65535"),
        ("-0o17", "-15"),
        ("+7", "7"),
        ("1_000", "1000"),
        ("1.5e3", "1.5e3"),
        ("+0.25e+1", "2.5e0"),
        ("#inf", "inf"),
        ("#nan", "nan"),
        ("1e400", "inf"),
        ("-1e400", "-inf"),
    ] {
        assert_eq!(plain::canonical_number(text), want, "{text}");
    }
    assert!(
        plain::canonical_number(&"9".repeat(50)).starts_with("beyond-i128:"),
        "wide integer"
    );
}

#[test]
fn every_valid_case_matches_and_every_invalid_case_but_hex_is_refused() {
    let expected_dir = corpus::root().join("expected_kdl");
    let mut failures = Vec::new();
    for (name, text) in corpus::inputs() {
        let expected = expected_dir.join(&name);
        match (expected.exists(), parse(&text, &ReadLimits::default())) {
            (true, Ok(nodes)) => {
                let want: kdl::KdlDocument =
                    fs::read_to_string(&expected).unwrap().parse().unwrap();
                let (got, want) = (
                    plain::normalise(plain::from_ours(&nodes)),
                    plain::normalise(plain::from_kdl(&want)),
                );
                if got != want {
                    failures.push(format!(
                        "{name}: tree differs\n  ours: {got:?}\n  want: {want:?}"
                    ));
                }
            }
            (true, Err(e)) => failures.push(format!("{name}: refused valid input: {e}")),
            // kdl-rs's corpus has no expected file for hex.kdl, yet `0xabcdef1234567890` is valid KDL.
            (false, Ok(_)) if name == "hex.kdl" => {}
            (false, Ok(_)) => failures.push(format!("{name}: accepted invalid input")),
            (false, Err(_)) if name == "hex.kdl" => {
                failures.push("hex.kdl: the documented exception is refused".into())
            }
            (false, Err(_)) => {}
        }
    }
    assert!(
        failures.is_empty(),
        "{} corpus cases fail:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
