//! Fixture round trips, the generator's round trip, and a board filled to every limit.

use placemat_core::board::{Access, Board, LIMITS, Warning, WriteError, load, write};

use crate::boardgen;
use crate::util::{all_kinds_board, fixture, limit, limits_board, rect};

const CANONICAL: [&str; 10] = [
    "arrow", "diamond", "ellipse", "empty", "escapes", "freedraw", "line", "rect", "text",
    "unknown",
];

#[test]
fn canonical_fixtures_round_trip_byte_identically() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/boards/canonical");
    let mut present: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .trim_end_matches(".board")
                .to_owned()
        })
        .collect();
    present.sort();
    assert_eq!(present, CANONICAL);
    for name in CANONICAL {
        let text = fixture(&format!("canonical/{name}.board"));
        assert!(!text.contains('\r'), "{name} has CR line endings");
        let loaded = load(text.as_bytes()).unwrap();
        assert_eq!(loaded.access, Access::ReadWrite, "{name}");
        assert!(loaded.warnings.is_empty(), "{name}: {:?}", loaded.warnings);
        assert_eq!(write(&loaded.board).unwrap(), text, "{name}");
    }
}

#[test]
fn the_kind_fixtures_are_the_all_kinds_board() {
    let mut joined = Board::new();
    for name in [
        "arrow", "diamond", "ellipse", "freedraw", "line", "rect", "text",
    ] {
        for e in load(fixture(&format!("canonical/{name}.board")).as_bytes())
            .unwrap()
            .board
            .elements()
        {
            joined.insert(e.clone()).unwrap();
        }
    }
    assert_eq!(joined, all_kinds_board());
}

#[test]
fn a_newer_fixture_opens_read_only_and_refuses_to_save() {
    let loaded = load(fixture("readonly/newer.board").as_bytes()).unwrap();
    assert_eq!(loaded.access, Access::ReadOnly { found: 1 });
    assert_eq!((loaded.board.version(), loaded.board.len()), (1, 1));
    assert!(
        loaded.board.contains_id("h"),
        "the unknown hologram lost its key"
    );
    assert_eq!(write(&loaded.board), Err(WriteError::ReadOnly { found: 1 }));
}

#[test]
fn generated_boards_round_trip_and_write_idempotently() {
    for s in 0..300 {
        let b = boardgen::board(&mut fastrand::Rng::with_seed(s));
        let text = write(&b).unwrap();
        let loaded = load(text.as_bytes()).unwrap();
        assert!(
            loaded.warnings.is_empty(),
            "seed {s}: {:?}",
            loaded.warnings
        );
        assert_eq!(loaded.board, b, "seed {s}");
        assert_eq!(write(&loaded.board).unwrap(), text, "seed {s}");
    }
}

#[test]
fn a_board_at_every_limit_stays_under_the_hard_cap_and_reopens() {
    let mut b = limits_board();
    assert_eq!(limit(b.insert(rect("over", 0, 0))), Err("elements"));
    let text = write(&b).unwrap();
    println!(
        "limits board: {} bytes, {} under the hard cap",
        text.len(),
        LIMITS.hard_bytes.saturating_sub(text.len())
    );
    assert!(text.len() <= LIMITS.hard_bytes, "{} bytes", text.len());
    let again = load(text.as_bytes()).unwrap();
    assert_eq!(
        again.warnings,
        [Warning::OverTeamLimit { bytes: text.len() }]
    );
    assert_eq!(again.board, b);
}
