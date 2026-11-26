//! The board editor: the view that owns an opened board, its camera and the canvas focus.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::{
    Bounds, Context, FocusHandle, InteractiveElement as _, IntoElement, Pixels, Render,
    Styled as _, TestSupportExt as _, Window, div,
};
use placemat_core::board::Board;
use placemat_core::scene::Camera;

use crate::viewer::open::OpenedBoard;

/// The view state the editor shares with the canvas element, which fits the camera once it
/// first learns the canvas bounds.
#[derive(Default)]
pub(crate) struct ViewState {
    /// The camera; None until the first frame fits the board to the canvas.
    pub(crate) camera: Cell<Option<Camera>>,
    /// The canvas bounds from the last frame, None before the first.
    #[expect(dead_code, reason = "the canvas element writes and reads it")]
    pub(crate) canvas: Cell<Option<Bounds<Pixels>>>,
}

/// A board opened in the window, viewed through a pan-and-zoom camera.
pub struct BoardEditor {
    opened: Rc<OpenedBoard>,
    view: Rc<ViewState>,
    focus: FocusHandle,
}

impl BoardEditor {
    /// An editor over `opened` with no camera yet: the first frame fits the board.
    pub fn new(opened: OpenedBoard, cx: &mut Context<Self>) -> Self {
        Self {
            opened: Rc::new(opened),
            view: Rc::new(ViewState::default()),
            focus: cx.focus_handle(),
        }
    }

    /// The board being viewed.
    pub fn board(&self) -> &Board {
        &self.opened.loaded.board
    }

    /// The stored camera, or the default camera before the first frame fits one.
    pub fn camera(&self) -> Camera {
        self.view.camera.get().unwrap_or_default()
    }

    /// Stores `camera` and repaints.
    pub fn set_camera(&mut self, camera: Camera, cx: &mut Context<Self>) {
        self.view.camera.set(Some(camera));
        cx.notify();
    }

    /// Clears the camera so the next frame fits the board to the canvas again.
    pub fn fit(&mut self, cx: &mut Context<Self>) {
        self.view.camera.set(None);
        cx.notify();
    }
}

impl Render for BoardEditor {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("board")
            .test_support()
            .track_focus(&self.focus)
            .key_context("BoardEditor")
            .size_full()
    }
}
