//! The perf fixtures, pinned to their generator, and a printed 10,000-element scan timing.

use std::path::{Path, PathBuf};
use std::time::Instant;

use placemat_core::board::{
    Arrow, ArrowEnd, Board, Boxed, Colour, Element, Fill, FillStyle, Kind, Path as Stroke, Point,
    Seed, Size, load, write,
};
use placemat_core::scene::{Rect, Scene};

use crate::util::{STUB, everything, id, z};

pub fn grid(n: usize) -> Board {
    let cols = (1..=n).find(|c| c * c >= n).unwrap_or(1);
    let mut b = Board::new();
    for i in 0..n {
        let x = i32::try_from(i % cols).unwrap() * 120;
        let y = i32::try_from(i / cols).unwrap() * 120;
        let mut body = Boxed::new(Point::new(x + 10, y + 20), Size::new(100, 80));
        if i % 3 == 0 {
            body.fill = Fill::Colour {
                colour: Colour::rgb(0xa5, 0xd8, 0xff),
                style: FillStyle::Hachure,
            };
        }
        let kind = match i % 5 {
            0 => Kind::Rect(body),
            1 => Kind::Ellipse(body),
            2 => {
                let mut walk = fastrand::Rng::with_seed(i as u64);
                let (mut points, mut dy) = (Vec::new(), 0);
                for k in 0..32 {
                    dy += walk.i32(-40..=40);
                    points.push(Point::new(k * 30, dy));
                }
                Kind::Freedraw(Stroke::new(Point::new(x + 10, y + 60), points))
            }
            3 => Kind::Arrow(Arrow::new(
                ArrowEnd::Free(Point::new(x + 10, y + 60)),
                ArrowEnd::Free(Point::new(x + 110, y + 60)),
            )),
            _ => {
                body.label = Some(format!("Label {i}"));
                Kind::Rect(body)
            }
        };
        let seed = Seed::new(u32::try_from(i).unwrap()).unwrap();
        b.insert(Element::new(id(&format!("g{i:05}")), seed, z("a1"), kind))
            .unwrap();
    }
    b
}

fn perf_path(n: usize) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/boards/perf/grid-{n}.board"))
}

#[test]
fn the_perf_fixtures_match_their_generator() {
    for n in [1000, 10_000] {
        let (path, text) = (perf_path(n), write(&grid(n)).unwrap());
        if std::env::var("PLACEMAT_BLESS").as_deref() == Ok("1") {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &text).unwrap();
        }
        let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "{}: {e}; bless it once with PLACEMAT_BLESS=1",
                path.display()
            )
        });
        assert!(
            committed == text,
            "grid-{n}.board drifted from its generator"
        );
        assert_eq!(load(committed.as_bytes()).unwrap().board.len(), n);
    }
}

#[test]
fn scan_benchmark() {
    let board = load(
        std::fs::read_to_string(perf_path(10_000))
            .unwrap()
            .as_bytes(),
    )
    .unwrap()
    .board;
    let scene = Scene::build(&board, &STUB);
    let all = scene.bounds().unwrap();
    let view = Rect::new(
        all.x0,
        all.y0,
        (all.x0 + all.x1) / 2.0,
        (all.y0 + all.y1) / 2.0,
    );
    let expected = scene
        .visible(everything())
        .filter(|v| v.bounds.intersects(&view))
        .count();
    assert!(
        expected > 0 && expected < 10_000,
        "{expected} in the quarter view"
    );
    let mut times = Vec::new();
    for _ in 0..101 {
        let start = Instant::now();
        let n = scene.visible(view).count();
        times.push(start.elapsed());
        assert_eq!(n, expected);
    }
    times.sort();
    println!(
        "scan of 10,000 elements, {expected} visible: median {:?}",
        times[50]
    );
}
