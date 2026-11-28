//! A committed hash of every canonical element's ops: a roughr, rand or sanitisation change fails it.

use std::fmt::Write as _;
use std::path::Path;

use placemat_core::board::load;
use placemat_core::scene::{Geometry, Op, Scene, SetKind};

use crate::board_util::fixture;
use crate::util::{STUB, everything};

const BOARDS: [&str; 11] = [
    "arrow", "diamond", "ellipse", "empty", "escapes", "freedraw", "heads", "line", "rect", "text",
    "unknown",
];

/// FNV-1a 64: no toolchain or dependency update can change its output.
fn fnv(h: u64, bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(h, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

fn hash(g: &Geometry) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325;
    for (pass, sets) in [(b'r', &g.rough), (b'o', &g.outline)] {
        h = fnv(h, &[pass]);
        for set in sets {
            let kind = match set.kind {
                SetKind::Stroke => b's',
                SetKind::Fill => b'f',
                SetKind::Sketch => b'k',
                SetKind::Solid => b'd',
                _ => panic!("a set kind the golden hash does not know"),
            };
            h = fnv(h, &[kind]);
            for op in &set.ops {
                let (tag, values) = match *op {
                    Op::Move(x, y) => (b'm', vec![x, y]),
                    Op::Line(x, y) => (b'l', vec![x, y]),
                    Op::Cubic(c) => (b'c', c.to_vec()),
                    _ => panic!("an op the golden hash does not know"),
                };
                h = fnv(h, &[tag]);
                for v in values {
                    h = fnv(h, &v.to_bits().to_le_bytes());
                }
            }
        }
    }
    fnv(h, &[u8::from(g.fallback)])
}

fn render() -> String {
    let mut out = String::new();
    for name in BOARDS {
        let board = load(fixture(&format!("canonical/{name}.board")).as_bytes())
            .unwrap()
            .board;
        for v in Scene::build(&board, &STUB).visible(everything()) {
            writeln!(out, "{name} {} {:016x}", v.id, hash(v.geometry)).unwrap();
        }
    }
    out
}

#[test]
fn the_hashed_boards_are_every_canonical_fixture() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/boards/canonical");
    let mut present: Vec<String> = std::fs::read_dir(dir)
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
    assert_eq!(present, BOARDS);
}

#[test]
fn building_the_same_boards_twice_gives_identical_ops() {
    assert_eq!(render(), render(), "two builds of the same boards differ");
}

// roughr's sin and cos go through the platform libm, so the exact bits are pinned on Linux only.
#[cfg(target_os = "linux")]
#[test]
fn canonical_ops_match_the_golden_hashes() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scene/golden.txt");
    let now = render();
    if std::env::var("PLACEMAT_BLESS").as_deref() == Ok("1") {
        std::fs::write(&path, &now).unwrap();
    }
    let golden = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e}; bless it once with PLACEMAT_BLESS=1",
            path.display()
        )
    });
    assert_eq!(
        now, golden,
        "roughr ops changed: a roughr, rand or sanitisation change is a format event"
    );
}
