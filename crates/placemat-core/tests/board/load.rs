//! The loader on known data: canonical and loose input, every error kind, the limits and read-only.

use placemat_core::board::{
    Access, Board, Kind, LIMITS, LoadError, Loaded, Seed, Warning, WriteError, load, write,
};

use crate::util::{ALL_KINDS, all_kinds_board, id, labelled};

const HEAD: &str = "placemat-board version=0\n\n";
const RECT: &str = "rect \"a\" {\n    at 0 0\n    seed 1\n    size 1 1\n    fill none\n    stroke \"#000000\" width=1 roughness=0 dash=solid\n    z \"a1\"\n}\n";

fn ok(text: &str) -> Loaded {
    load(text.as_bytes()).unwrap()
}

fn err(text: &str) -> LoadError {
    load(text.as_bytes()).unwrap_err()
}

/// The one-rect board with its first `from` replaced by `to`.
fn edited(from: &str, to: &str) -> String {
    let rect = RECT.replacen(from, to, 1);
    assert_ne!(rect, RECT, "{from:?} is not in the rect");
    format!("{HEAD}{rect}")
}

fn invalid_at(e: LoadError) -> Option<(Option<String>, u32, u32)> {
    if let LoadError::Invalid { id, line, col, .. } = e {
        return Some((id.map(|i| i.as_str().to_owned()), line, col));
    }
    None
}

#[test]
fn the_canonical_board_loads_to_its_model() {
    let loaded = ok(ALL_KINDS);
    assert_eq!(loaded.board, all_kinds_board());
    assert_eq!(loaded.access, Access::ReadWrite);
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert_eq!(write(&loaded.board).unwrap(), ALL_KINDS);
}

#[test]
fn loose_input_is_written_canonically() {
    let loose = "\u{feff}placemat-board version=0 // header\r\n\
        text \"t\" { z \"a1\"; stroke \"#1E1E1E\" dash=solid roughness=1.0 width=2; font align=left size=20 family=hand\n\
        content \"hi\" /* drop me */; seed +5; at 007 -0 }\n";
    let want = "placemat-board version=0\n\ntext \"t\" {\n    at 7 0\n    seed 5\n    content \"hi\"\n    font family=hand size=20 align=left\n    stroke \"#1e1e1e\" width=2 roughness=1 dash=solid\n    z \"a1\"\n}\n";
    assert_eq!(write(&ok(loose).board).unwrap(), want);
}

#[test]
fn byte_level_failures() {
    assert_eq!(
        load(&vec![b' '; LIMITS.hard_bytes + 1]),
        Err(LoadError::TooLarge)
    );
    assert_eq!(
        load(&vec![b' '; LIMITS.hard_bytes]),
        Err(LoadError::NotABoard)
    );
    assert_eq!(
        load(b"placemat-board version=0\n\xff"),
        Err(LoadError::NotUtf8)
    );
}

#[test]
fn a_missing_or_malformed_header_is_not_a_board() {
    for text in [
        "",
        "\n",
        "rect \"a\" {}",
        "placemat-board",
        "placemat-board version=\"0\"",
        "placemat-board version=-1",
        "placemat-board version=0.5",
        "placemat-board version=99999999999",
        "other version=0",
        "placemat-board version=0 version=0",
        "placemat-board version=0 { x }",
    ] {
        assert_eq!(err(text), LoadError::NotABoard, "{text:?}");
    }
}

#[test]
fn syntax_errors_carry_a_position() {
    let cut = format!("{HEAD}rect \"a\" {{\n    at 0 0\n");
    let want = LoadError::Syntax {
        line: 5,
        col: 1,
        msg: "unexpected end of input".to_owned(),
    };
    assert_eq!(err(&cut), want);
    let deep = edited(
        "    z \"a1\"\n",
        "    z \"a1\"\n    a { b { c { d { e } } } }\n",
    );
    assert!(matches!(err(&deep), LoadError::Syntax { .. }), "{deep}");
}

#[test]
fn hostile_and_malformed_values_are_invalid() {
    assert_eq!(
        invalid_at(err(&edited("at 0 0", "at 1e400 0"))),
        Some((Some("a".to_owned()), 4, 8))
    );
    for (from, to) in [
        ("at 0 0", "at #nan 0"),
        ("at 0 0", "at #inf 0"),
        ("at 0 0", "at 0.5 0"),
        ("at 0 0", "at 16777217 0"),
        ("at 0 0", "at 0x10 0"),
        ("at 0 0", "at \"0\" 0"),
        ("at 0 0", "(px)at 0 0"),
        ("at 0 0", "at (px)0 0"),
        (
            "seed 1",
            "seed 12345678901234567890123456789012345678901234567890",
        ),
        ("seed 1", "seed -1"),
        ("seed 1", "seed 18446744073709551616"),
        ("size 1 1", "size 1"),
        ("width=1", "width=100.1"),
        ("width=1", "width=0.05"),
        ("roughness=0", "roughness=10.1"),
        ("dash=solid", "dash=wavy"),
        ("#000000", "#00000"),
        ("fill none", "fill none style=solid"),
        ("fill none", "fill \"#ff0000\""),
        ("z \"a1\"", "z \"a1\" { x }"),
    ] {
        let text = edited(from, to);
        assert!(
            invalid_at(err(&text)).is_some_and(|(i, ..)| i.as_deref() == Some("a")),
            "{text}"
        );
    }
}

#[test]
fn repeated_missing_and_unidentified_lines_are_invalid() {
    assert_eq!(
        invalid_at(err(&edited("seed 1", "seed 1\n    seed 2"))),
        Some((Some("a".to_owned()), 6, 5))
    );
    assert_eq!(
        invalid_at(err(&edited("width=1", "width=1 width=2"))),
        Some((Some("a".to_owned()), 8, 30))
    );
    assert_eq!(
        invalid_at(err(&edited("    seed 1\n", ""))),
        Some((Some("a".to_owned()), 3, 1))
    );
    for (from, to) in [
        ("rect \"a\" {", "rect \"a b\" {"),
        ("rect \"a\" {", "rect 5 {"),
        ("rect \"a\" {", "rect {"),
    ] {
        let text = edited(from, to);
        assert!(
            invalid_at(err(&text)).is_some_and(|(i, ..)| i.is_none()),
            "{text}"
        );
    }
    assert!(invalid_at(err(&format!("{HEAD}rect \"a\"\n"))).is_some());
}

#[test]
fn every_count_limit_plus_one_is_refused() {
    let many = |n: usize| -> String {
        let blocks: Vec<String> = (0..n)
            .map(|i| RECT.replacen("\"a\"", &format!("\"e{i}\""), 1))
            .collect();
        format!("{HEAD}{}", blocks.join("\n"))
    };
    assert_eq!(ok(&many(LIMITS.elements)).board.len(), LIMITS.elements);
    assert_eq!(
        err(&many(LIMITS.elements + 1)),
        LoadError::OverLimit {
            limit: "elements",
            id: None
        }
    );
    let pen = |name: &str, n: usize| {
        format!(
            "\nfreedraw \"{name}\" {{\n    at 0 0\n    seed 1\n    points{}\n    stroke \"#000000\" width=1 roughness=0 dash=solid\n    z \"a1\"\n}}\n",
            " 1 1".repeat(n)
        )
    };
    let one = format!("{HEAD}{}", pen("f", LIMITS.points_per_element + 1));
    assert_eq!(
        err(&one),
        LoadError::OverLimit {
            limit: "points_per_element",
            id: Some(id("f"))
        }
    );
    let full: String = (0..LIMITS.points_total / LIMITS.points_per_element)
        .map(|i| pen(&format!("f{i}"), LIMITS.points_per_element))
        .collect();
    assert_eq!(
        err(&format!("{HEAD}{full}{}", pen("g", 1))),
        LoadError::OverLimit {
            limit: "points_total",
            id: Some(id("g"))
        }
    );
    let label = |n: usize| edited("    z", &format!("    label \"{}\"\n    z", "a".repeat(n)));
    assert_eq!(
        err(&label(LIMITS.string_bytes_each + 1)),
        LoadError::OverLimit {
            limit: "string_bytes_each",
            id: Some(id("a"))
        }
    );
    let mut b = Board::new();
    for i in 0..LIMITS.string_bytes_total / LIMITS.string_bytes_each {
        b.insert(labelled(
            &format!("l{i}"),
            "a".repeat(LIMITS.string_bytes_each),
        ))
        .unwrap();
    }
    let over = format!(
        "{}\n{}",
        write(&b).unwrap(),
        label(1).replacen(HEAD, "", 1).replacen("\"a\"", "\"m\"", 1)
    );
    assert_eq!(
        err(&over),
        LoadError::OverLimit {
            limit: "string_bytes_total",
            id: Some(id("m"))
        }
    );
}

#[test]
fn a_board_over_the_team_limit_loads_with_a_warning() {
    let mut b = Board::new();
    for i in 0..LIMITS.string_bytes_total / LIMITS.string_bytes_each {
        b.insert(labelled(
            &format!("l{i}"),
            "a".repeat(LIMITS.string_bytes_each),
        ))
        .unwrap();
    }
    let text = write(&b).unwrap();
    assert!(text.len() > LIMITS.team_bytes, "{} bytes", text.len());
    let loaded = ok(&text);
    assert_eq!(
        loaded.warnings,
        [Warning::OverTeamLimit { bytes: text.len() }]
    );
    assert_eq!(loaded.board, b);
}

#[test]
fn out_of_range_seeds_clamp_and_bad_z_keys_sort_last() {
    let loaded = ok(&edited("seed 1", "seed 4294967296"));
    assert_eq!(loaded.board.get(&id("a")).unwrap().seed.get(), Seed::MAX);
    assert_eq!(loaded.warnings, [Warning::SeedClamped { id: id("a") }]);
    let both = edited("seed 1", "seed 4294967296").replacen("z \"a1\"", "z \"a0\"", 1);
    let loaded = ok(&both);
    assert_eq!(
        loaded.warnings,
        [
            Warning::SeedClamped { id: id("a") },
            Warning::InvalidZ { id: id("a") }
        ]
    );
    let e = loaded.board.get(&id("a")).unwrap();
    assert!(!e.z.is_valid() && e.z.as_str().is_empty(), "{:?}", e.z);
    assert!(
        write(&loaded.board).unwrap().contains("\n    z \"\"\n"),
        "invalid key not written empty"
    );
    assert!(matches!(e.kind, Kind::Rect(_)), "{:?}", e.kind);
}

#[test]
fn a_newer_version_opens_read_only() {
    let newer = ALL_KINDS.replacen("version=0", "version=1", 1);
    let loaded = ok(&newer);
    assert_eq!(loaded.access, Access::ReadOnly { found: 1 });
    assert_eq!(loaded.board.version(), 1);
    assert_eq!(write(&loaded.board), Err(WriteError::ReadOnly { found: 1 }));
    assert!(invalid_at(err(&newer.replacen("seed 0", "seed #nan", 1))).is_some());
}
