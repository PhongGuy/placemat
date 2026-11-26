//! Opening a board file into a scene, the work the background executor does before the UI sees it.

use std::path::PathBuf;
use std::sync::Arc;

use placemat_core::board::{Loaded, OpenError, read_file};
use placemat_core::scene::{Scene, TextMeasure};

/// A board read from disk with its scene built, ready to hand to the UI thread.
#[derive(Debug)]
#[non_exhaustive]
pub struct OpenedBoard {
    /// The path the board was read from.
    pub path: PathBuf,
    /// The loaded board, its warnings and whether it may be saved.
    pub loaded: Loaded,
    /// The scene built from the board.
    pub scene: Scene,
}

/// Reads, loads and builds the scene; runs on the background executor.
pub fn open_board(path: PathBuf, m: Arc<dyn TextMeasure>) -> Result<OpenedBoard, OpenError> {
    let loaded = read_file(&path)?;
    let scene = Scene::build(&loaded.board, m.as_ref());
    Ok(OpenedBoard {
        path,
        loaded,
        scene,
    })
}
