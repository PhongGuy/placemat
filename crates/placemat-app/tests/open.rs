//! Opening a board file into a built scene, and the failures that leave the app as it was.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::{NoopTextSystem, TextSystem};
use placemat_app::viewer::measure::{GpuiMeasure, LINE_HEIGHT_EM, advance_sum};
use placemat_app::viewer::open::{OpenedBoard, open_board};
use placemat_core::board::OpenError;
use placemat_core::scene::TextMeasure;
use placemat_core::scene::{FixedAdvance, Rect};

const STUB: FixedAdvance = FixedAdvance::new(0.5, 1.25);

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../placemat-core/tests/fixtures/boards")
        .join(rel)
}

#[test]
fn a_board_file_opens_with_its_scene_built() {
    let path = fixture("perf/grid-1000.board");
    let opened = open_board(path.clone(), Arc::new(STUB)).unwrap();
    assert_eq!(opened.path, path);
    assert_eq!(opened.loaded.board.len(), 1000);
    let everything = Rect::new(-1e12, -1e12, 1e12, 1e12);
    assert_eq!(opened.scene.visible(everything).count(), 1000);
}

#[test]
fn a_missing_path_is_an_io_error() {
    let r = open_board(fixture("no-such.board"), Arc::new(STUB));
    assert!(matches!(r, Err(OpenError::Io(_))), "{:?}", r.err());
}

#[test]
fn a_directory_is_not_a_file() {
    let r = open_board(fixture("canonical"), Arc::new(STUB));
    assert!(matches!(r, Err(OpenError::NotAFile)), "{:?}", r.err());
}

#[test]
fn text_that_is_not_a_board_is_a_load_error() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("broken.board");
    std::fs::write(&path, "not a board").unwrap();
    let r = open_board(path, Arc::new(STUB));
    assert!(matches!(r, Err(OpenError::Load(_))), "{:?}", r.err());
}

#[test]
fn the_gpui_measurer_sums_advances_and_uses_the_line_height() {
    // NoopTextSystem: a BMP char advances 0.6 em, a char outside it 1.2 em.
    let m = GpuiMeasure::new(Arc::new(TextSystem::new(Arc::new(NoopTextSystem::new()))));
    assert!(
        (m.line_width("ab", 10.0) - 12.0).abs() < 1e-4,
        "{}",
        m.line_width("ab", 10.0)
    );
    assert!((m.line_width("\u{1F600}", 10.0) - 12.0).abs() < 1e-4);
    assert_eq!(m.line_width("", 10.0), 0.0);
    assert_eq!(m.line_height(20.0), LINE_HEIGHT_EM * 20.0);
}

#[test]
fn a_char_with_no_advance_counts_half_an_em() {
    let advance = |c: char| (c != '?').then_some(3.0);
    assert_eq!(advance_sum("a?b", 10.0, advance), 3.0 + 5.0 + 3.0);
}

#[test]
fn an_opened_board_crosses_threads_and_the_measurer_is_shared() {
    fn send<T: Send>() {}
    fn send_sync<T: Send + Sync>() {}
    send::<OpenedBoard>();
    send_sync::<GpuiMeasure>();
}
