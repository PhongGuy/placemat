//! Element values: ids, seeds, z keys, colours and constructor defaults.

use placemat_core::board::{
    Align, Arrow, ArrowEnd, Colour, Dash, ElementId, Fill, FontFamily, Head, Kind, Point, Seed,
    Text, ZKey,
};

use crate::util::{id, rect, z};

#[test]
fn element_ids_are_1_to_32_url_safe_ascii_chars() {
    let (longest, too_long) = ("x".repeat(32), "x".repeat(33));
    for ok in ["a", "Z9_-", longest.as_str()] {
        assert!(ElementId::new(ok).is_some(), "{ok}");
    }
    for bad in ["", too_long.as_str(), "a b", "a.b", "é", "a\"b"] {
        assert!(ElementId::new(bad).is_none(), "{bad:?}");
    }
    assert_eq!(id("3kTq8wZp").as_str(), "3kTq8wZp");
    assert_eq!(id("3kTq8wZp").to_string(), "3kTq8wZp");
}

#[test]
fn seeds_are_31_bit() {
    assert_eq!(Seed::MAX, 2_147_483_647);
    assert_eq!(Seed::new(Seed::MAX).map(Seed::get), Some(Seed::MAX));
    assert_eq!(Seed::new(Seed::MAX + 1), None);
}

#[test]
fn z_keys_are_base62_without_a_trailing_zero() {
    let (longest, too_long) = ("z".repeat(64), "z".repeat(65));
    for ok in ["a1", "A", "0a", longest.as_str()] {
        assert!(
            ZKey::new(ok).is_some_and(|k| k.is_valid() && k.as_str() == ok),
            "{ok}"
        );
    }
    for bad in ["", "a0", "0", "a-1", "a_", too_long.as_str(), "é"] {
        assert!(ZKey::new(bad).is_none(), "{bad:?}");
    }
}

#[test]
fn colours_parse_hex_and_print_lowercase() {
    assert_eq!(
        Colour::parse("#A5D8FF").map(|c| c.to_string()),
        Some("#a5d8ff".to_owned())
    );
    assert_eq!(
        Colour::parse("#a5d8ff80"),
        Some(Colour::rgba(0xa5, 0xd8, 0xff, 0x80))
    );
    assert_eq!(Colour::rgb(0x1e, 0x1e, 0x1e).to_string(), "#1e1e1e");
    assert_eq!(Colour::rgba(0, 0, 0, 0xff).to_string(), "#000000ff");
    for bad in ["a5d8ff", "#12345", "#gggggg", "#a5d8ff8", "", "#a5d8ff800"] {
        assert_eq!(Colour::parse(bad), None, "{bad:?}");
    }
}

#[test]
fn new_elements_take_the_documented_defaults() {
    let e = rect("a", 1, 2);
    assert_eq!(e.stroke.colour, Colour::rgb(0x1e, 0x1e, 0x1e));
    assert_eq!(
        (e.stroke.width, e.stroke.roughness, e.stroke.dash),
        (20, 10, Dash::Solid)
    );
    let Kind::Rect(boxed) = &e.kind else {
        panic!("not a rect")
    };
    assert_eq!((boxed.fill, boxed.label.as_deref()), (Fill::None, None));
    let text = Text::new(Point::new(0, 0), String::new());
    assert_eq!(
        (text.font.family, text.font.size, text.font.align),
        (FontFamily::Hand, 200, Align::Left)
    );
    let arrow = Arrow::new(
        ArrowEnd::Free(Point::new(0, 0)),
        ArrowEnd::Free(Point::new(1, 1)),
    );
    assert_eq!(
        (arrow.heads.start, arrow.heads.end, arrow.label),
        (Head::None, Head::Arrow, None)
    );
}

#[test]
fn z_keys_compare_by_bytes() {
    let mut keys = ["b", "a1", "a", "Z", "9", "a01"].map(z).to_vec();
    keys.sort();
    let got: Vec<&str> = keys.iter().map(ZKey::as_str).collect();
    assert_eq!(got, ["9", "Z", "a", "a01", "a1", "b"]);
}
