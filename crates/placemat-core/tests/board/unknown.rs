//! Unknown header entries, line entries, lines and nodes survive a load and land in canonical places.

use placemat_core::board::{LIMITS, LoadError, load, write};

use crate::util::id;

/// Canonical form of every unknown-data position; also the `canonical/unknown.board` fixture.
pub const WANT: &str = r##"placemat-board version=0 future=1 "pos"

widget "a" {
    x 1
}

zeta "b" size=2

rect "c" hint=1 {
    at 0 0
    seed 3
    size 1 1 "extra"
    fill none
    stroke "#000000" width=1 roughness=0 dash=solid glow=#true
    future-line 1 {
        nested #true
    }
    z "a1"
}

rect "d" {
    at 0 0
    seed 4
    size 1 1
    fill none
    stroke "#000000" width=1 roughness=0 dash=solid
    later "x"
    z "a2"
}

loose 1
"##;

const LOOSE: &str = r##"placemat-board version=0 future=1 "pos"
zeta "b" size=2
rect "c" hint=1 {
    future-line 1 { nested #true; }
    z "a1"
    stroke "#000000" width=1 roughness=0 dash=solid glow=#true
    fill none
    size 1 1 extra
    seed 3
    at 0 0
}
widget "a" { x 1 }
loose 1
rect "d" {
    at 0 0
    seed 4
    size 1 1
    fill none
    stroke "#000000" width=1 roughness=0 dash=solid
    z "a2"
    later x
}
"##;

#[test]
fn unknown_data_is_written_in_canonical_places() {
    let loaded = load(LOOSE.as_bytes()).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert_eq!(write(&loaded.board).unwrap(), WANT);
    let again = load(WANT.as_bytes()).unwrap();
    assert_eq!(again.board, loaded.board);
    assert_eq!(write(&again.board).unwrap(), WANT);
}

#[test]
fn unknown_keys_share_the_id_namespace() {
    let board = load(WANT.as_bytes()).unwrap().board;
    assert!(
        board.contains_id("a") && board.contains_id("b") && board.contains_id("c"),
        "{board:?}"
    );
    assert!(!board.contains_id("loose"), "an unkeyed node has no id");
    assert_eq!(board.len(), 2);
}

#[test]
fn numbers_in_unknown_data_keep_their_source_text() {
    let text = "placemat-board version=0\n\nw \"k\" 0x1F 1_000 12345678901234567890123456789012345678901234567890 1e400 #nan\n";
    assert_eq!(write(&load(text.as_bytes()).unwrap().board).unwrap(), text);
}

#[test]
fn unknown_content_past_its_limit_is_refused() {
    let blob = "a".repeat(600 << 10);
    let text =
        format!("placemat-board version=0\n\nw \"one\" b=\"{blob}\"\n\nw \"two\" b=\"{blob}\"\n");
    assert_eq!(
        load(text.as_bytes()),
        Err(LoadError::OverLimit {
            limit: "unknown_bytes_total",
            id: None
        })
    );
}

#[test]
fn unknown_bytes_count_as_written() {
    // Each one-byte unknown line writes as "    x\n", six bytes.
    let max = LIMITS.unknown_bytes_total / 6;
    let with = |n: usize| {
        format!(
            "placemat-board version=0\n\nrect \"a\" {{\n    at 0 0\n    seed 1\n    size 1 1\n    fill none\n    stroke \"#000000\" width=1 roughness=0 dash=solid\n{}    z \"a1\"\n}}\n",
            "    x\n".repeat(n)
        )
    };
    let full = load(with(max).as_bytes()).unwrap().board;
    assert_eq!(
        load(with(max + 1).as_bytes()),
        Err(LoadError::OverLimit {
            limit: "unknown_bytes_total",
            id: None
        })
    );
    let one = load(with(1).as_bytes())
        .unwrap()
        .board
        .get(&id("a"))
        .unwrap()
        .clone();
    let mut b = full.clone();
    let mut other = one.clone();
    other.id = id("b");
    assert_eq!(
        b.insert(other).map_err(|e| e.limit),
        Err("unknown_bytes_total")
    );
    b.insert(one).unwrap();
}
