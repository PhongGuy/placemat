//! Load-time repair: duplicate ids, dangling bindings, and the order warnings arrive in.

use placemat_core::board::{End, Warning, load, write};

use crate::util::{fixture, id};

/// Loads `repair/<name>.board`, checks it writes `<name>.expected.board` and that the
/// expected file is itself stable, and returns the load's warnings.
fn repaired(name: &str) -> Vec<Warning> {
    let loaded = load(fixture(&format!("repair/{name}.board")).as_bytes()).unwrap();
    let expected = fixture(&format!("repair/{name}.expected.board"));
    assert_eq!(write(&loaded.board).unwrap(), expected, "{name}");
    let again = load(expected.as_bytes()).unwrap();
    assert_eq!(
        write(&again.board).unwrap(),
        expected,
        "{name} is not stable"
    );
    loaded.warnings
}

fn rect_block(i: &str) -> String {
    format!(
        "rect \"{i}\" {{\n    at 0 0\n    seed 1\n    size 1 1\n    fill none\n    stroke \"#000000\" width=1 roughness=0 dash=solid\n    z \"a1\"\n}}\n"
    )
}

#[test]
fn later_duplicates_are_renamed_and_bindings_stay_with_the_first() {
    assert_eq!(
        repaired("duplicate"),
        [
            Warning::DuplicateId {
                original: id("a1Xq"),
                renamed: id("8nZl40fQKON")
            },
            Warning::DuplicateId {
                original: id("a1Xq"),
                renamed: id("8nZlNMpj4Ju")
            },
            Warning::DanglingBinding {
                arrow: id("k4Ar"),
                end: End::To,
                target: id("zz9")
            },
            Warning::SeedClamped {
                id: id("8nZl40fQKON")
            },
            Warning::InvalidZ {
                id: id("8nZlNMpj4Ju")
            },
        ]
    );
}

#[test]
fn dangling_ends_collapse_onto_the_surviving_end() {
    assert_eq!(
        repaired("dangling"),
        [
            Warning::DanglingBinding {
                arrow: id("k4Ar"),
                end: End::From,
                target: id("b7Kp")
            },
            Warning::DanglingBinding {
                arrow: id("m2Zz"),
                end: End::To,
                target: id("gone")
            },
            Warning::DanglingBinding {
                arrow: id("n3Yy"),
                end: End::From,
                target: id("x")
            },
            Warning::DanglingBinding {
                arrow: id("n3Yy"),
                end: End::To,
                target: id("y")
            },
        ]
    );
}

#[test]
fn an_invalid_z_key_paints_last() {
    assert_eq!(repaired("invalid-z"), [Warning::InvalidZ { id: id("a") }]);
    let board = load(fixture("repair/invalid-z.board").as_bytes())
        .unwrap()
        .board;
    let order: Vec<&str> = board.paint_order().map(|e| e.id.as_str()).collect();
    assert_eq!(order, ["b", "a"]);
}

#[test]
fn a_rename_skips_ids_used_later_in_the_file() {
    let text = format!(
        "placemat-board version=0\n\n{}\n{}\n{}",
        rect_block("a1Xq"),
        rect_block("a1Xq"),
        rect_block("8nZl40fQKON")
    );
    let loaded = load(text.as_bytes()).unwrap();
    assert_eq!(
        loaded.warnings,
        [Warning::DuplicateId {
            original: id("a1Xq"),
            renamed: id("8nZlNMpj4Ju")
        }]
    );
    let ids: Vec<&str> = loaded.board.elements().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["8nZl40fQKON", "8nZlNMpj4Ju", "a1Xq"]);
}

#[test]
fn an_unknown_node_claims_its_id_first() {
    let unknown_first = format!(
        "placemat-board version=0\n\nwidget \"a1Xq\"\n\n{}",
        rect_block("a1Xq")
    );
    let loaded = load(unknown_first.as_bytes()).unwrap();
    assert_eq!(
        loaded.warnings,
        [Warning::DuplicateId {
            original: id("a1Xq"),
            renamed: id("8nZl40fQKON")
        }]
    );
    let element_first = format!(
        "placemat-board version=0\n\n{}\nwidget \"a1Xq\"\n",
        rect_block("a1Xq")
    );
    let loaded = load(element_first.as_bytes()).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert_eq!(write(&loaded.board).unwrap(), element_first);
}

#[test]
fn a_collapsed_end_is_clamped_into_the_coordinate_range() {
    let text = "placemat-board version=0\n\nrect \"big\" {\n    at 16777216 10\n    seed 1\n    size 16777216 -3\n    fill none\n    stroke \"#000000\" width=1 roughness=0 dash=solid\n    z \"a1\"\n}\n\narrow \"k\" {\n    from \"big\" x=1 y=0.5\n    seed 2\n    to \"gone\" x=0 y=0\n    head start=none end=arrow\n    stroke \"#000000\" width=1 roughness=0 dash=solid\n    z \"a2\"\n}\n";
    let written = write(&load(text.as_bytes()).unwrap().board).unwrap();
    // x: 2^24 + 2^24 clamps to 2^24; y: 10 + floor(-3 * 0.5) = 8.
    assert!(written.contains("\n    to 16777216 8\n"), "{written}");
    assert!(
        load(written.as_bytes()).is_ok(),
        "the repaired board does not reopen"
    );
}

#[test]
fn repair_is_the_same_on_every_open() {
    let text = fixture("repair/duplicate.board");
    assert_eq!(
        load(text.as_bytes()).unwrap(),
        load(text.as_bytes()).unwrap()
    );
}
