//! The colour shown for a stored colour: the identity in light, Excalidraw's canvas filter
//! (`invert(93%) hue-rotate(180deg)`) in dark, with exact bytes worked by hand.

use placemat_core::board::{Appearance, Colour};

fn hex(s: &str) -> Colour {
    Colour::parse(s).unwrap()
}

/// (stored, shown in dark), every value checked by hand from the rule in the spec.
const DARK: [(&str, &str); 8] = [
    ("#1e1e1e", "#d3d3d3"),
    ("#000000", "#ededed"),
    ("#ffffff", "#121212"),
    ("#e03131", "#ff8383"),
    ("#1971c2", "#56a2e8"),
    ("#f08c00", "#b76100"),
    ("#a5d8ff", "#154163"),
    ("#060608", "#e8e8e9"),
];

#[test]
fn light_shows_every_colour_as_stored() {
    for (stored, _) in DARK {
        assert_eq!(
            hex(stored).shown(Appearance::Light),
            hex(stored),
            "{stored}"
        );
    }
    for stored in ["#1e1e1e80", "#a5d8ff00", "#e03131ff"] {
        assert_eq!(
            hex(stored).shown(Appearance::Light),
            hex(stored),
            "{stored}"
        );
    }
}

#[test]
fn dark_inverts_to_93_percent_and_turns_the_hue_half_way() {
    for (stored, shown) in DARK {
        assert_eq!(
            hex(stored).shown(Appearance::Dark).to_string(),
            shown,
            "{stored}"
        );
    }
}

#[test]
fn dark_clamps_channels_that_leave_the_unit_range() {
    // Red's red channel lands at 281.4 before the clamp, orange's blue at -22.9.
    assert_eq!(
        Colour::rgb(0xe0, 0x31, 0x31).shown(Appearance::Dark),
        Colour::rgb(0xff, 0x83, 0x83)
    );
    assert_eq!(
        Colour::rgb(0xf0, 0x8c, 0x00).shown(Appearance::Dark),
        Colour::rgb(0xb7, 0x61, 0x00)
    );
}

#[test]
fn dark_rounds_a_channel_just_under_a_half_down() {
    // Blue's red channel is 86.4986 before rounding: 86, never 87.
    assert_eq!(
        Colour::rgb(0x19, 0x71, 0xc2)
            .shown(Appearance::Dark)
            .channels(),
        [0x56, 0xa2, 0xe8, 0xff]
    );
}

#[test]
fn dark_keeps_the_alpha_and_the_digit_count_as_stored() {
    assert_eq!(
        hex("#1e1e1e80").shown(Appearance::Dark).to_string(),
        "#d3d3d380"
    );
    assert_eq!(
        hex("#a5d8ff00").shown(Appearance::Dark).to_string(),
        "#15416300"
    );
    assert_eq!(
        hex("#000000ff").shown(Appearance::Dark).to_string(),
        "#edededff"
    );
    assert_eq!(
        hex("#1e1e1e").shown(Appearance::Dark).to_string(),
        "#d3d3d3"
    );
}

#[test]
fn every_grey_stays_grey_and_no_byte_panics() {
    // Each matrix row sums to one, so a grey input keeps r == g == b.
    for v in 0..=u8::MAX {
        let [r, g, b, a] = Colour::rgb(v, v, v).shown(Appearance::Dark).channels();
        assert!(r == g && g == b && a == 0xff, "{v}: {r} {g} {b} {a}");
    }
}
