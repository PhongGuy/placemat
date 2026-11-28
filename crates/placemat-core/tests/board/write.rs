//! The canonical writer: layout, ordering, number forms and string escaping.

use placemat_core::board::{Board, Element, load, write};

use crate::util::{ALL_KINDS, all_kinds_board, labelled};

#[test]
fn writes_every_kind_in_canonical_form() {
    assert_eq!(write(&all_kinds_board()).unwrap(), ALL_KINDS);
}

#[test]
fn an_empty_board_is_one_header_line() {
    assert_eq!(write(&Board::new()).unwrap(), "placemat-board version=0\n");
}

#[test]
fn insertion_order_does_not_change_the_bytes() {
    let elements: Vec<Element> = all_kinds_board().elements().cloned().collect();
    let mut reversed = Board::new();
    for e in elements.into_iter().rev() {
        reversed.insert(e).unwrap();
    }
    assert_eq!(write(&reversed).unwrap(), ALL_KINDS);
}

#[test]
fn free_strings_escape_controls_and_bidi() {
    let label = "q\"b\\s\nn\rr\tt\u{8}b\u{c}f\u{0}\u{1b}\u{7f}\u{85}\u{200e}\u{200f}\u{202a}\u{202e}\u{2066}\u{2069}\u{2028}\u{2029}\u{feff}é漢🎉";
    let mut b = Board::new();
    b.insert(labelled("a", label.to_owned())).unwrap();
    let text = write(&b).unwrap();
    let want = r#"    label "q\"b\\s\nn\rr\tt\bb\ff\u{0}\u{1b}\u{7f}\u{85}\u{200e}\u{200f}\u{202a}\u{202e}\u{2066}\u{2069}\u{2028}\u{2029}\u{feff}é漢🎉""#;
    assert!(text.lines().any(|l| l == want), "{text}");
    assert!(!text.contains('\t') && !text.contains('\r'), "{text:?}");
}

#[test]
fn an_old_raw_hidden_character_loads_unchanged_and_writes_escaped() {
    let mut b = Board::new();
    b.insert(labelled("a", "xy".to_owned())).unwrap();
    let old = write(&b).unwrap().replace("\"xy\"", "\"x\u{200b}y\"");
    let loaded = load(old.as_bytes()).unwrap();
    let mut want = Board::new();
    want.insert(labelled("a", "x\u{200b}y".to_owned())).unwrap();
    assert_eq!(loaded.board, want);
    let text = write(&loaded.board).unwrap();
    assert!(text.contains(r#"label "x\u{200b}y""#), "{text}");
}
