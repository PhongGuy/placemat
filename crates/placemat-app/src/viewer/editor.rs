//! The board editor: the view that owns an opened board, its camera and the canvas focus.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::{
    Bounds, Context, FocusHandle, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, ParentElement as _, Pixels, Point, Render, ScrollEvent,
    Styled as _, TestSupportExt as _, Window, div,
};
use placemat_core::board::Board;
use placemat_core::scene::Camera;

use crate::viewer::FitBoard;
use crate::viewer::element::BoardElement;
use crate::viewer::input::{Wheel, wheel};
use crate::viewer::open::OpenedBoard;

/// The view state the editor shares with the canvas element, which fits the camera once it
/// first learns the canvas bounds.
#[derive(Default)]
pub(crate) struct ViewState {
    /// The camera; None until the first frame fits the board to the canvas.
    pub(crate) camera: Cell<Option<Camera>>,
    /// The canvas bounds from the last frame, None before the first.
    pub(crate) canvas: Cell<Option<Bounds<Pixels>>>,
}

/// A board opened in the window, viewed through a pan-and-zoom camera.
pub struct BoardEditor {
    opened: Rc<OpenedBoard>,
    view: Rc<ViewState>,
    focus: FocusHandle,
    /// The pointer position at the last step of a left-button drag, None when no drag is live.
    drag: Option<Point<Pixels>>,
}

impl BoardEditor {
    /// An editor over `opened` with no camera yet: the first frame fits the board.
    pub fn new(opened: OpenedBoard, cx: &mut Context<Self>) -> Self {
        Self {
            opened: Rc::new(opened),
            view: Rc::new(ViewState::default()),
            focus: cx.focus_handle(),
            drag: None,
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

    /// Applies `change` to the stored camera; before the first frame fits one, input does nothing.
    fn change_camera(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Camera)) {
        if let Some(mut camera) = self.view.camera.get() {
            change(&mut camera);
            self.set_camera(camera, cx);
        }
    }

    /// A left press focuses the board, so its keys reach it, and starts a drag.
    fn press(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        self.drag = Some(event.position);
    }

    /// Pans by the pointer's step while the left button stays down; a move without it ends
    /// the drag, which covers a release the board never saw.
    fn drag_to(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(last) = self.drag.filter(|_| event.dragging()) else {
            self.drag = None;
            return;
        };
        self.drag = Some(event.position);
        let step = event.position - last;
        self.change_camera(cx, |c| {
            c.pan(f64::from(f32::from(step.x)), f64::from(f32::from(step.y)));
        });
    }

    /// The wheel zooms about the cursor, or pans with Shift.
    fn scroll(&mut self, event: &ScrollEvent, cx: &mut Context<Self>) {
        let Some(canvas) = self.view.canvas.get() else {
            return;
        };
        let at = event.position - canvas.origin;
        let cursor = (f64::from(f32::from(at.x)), f64::from(f32::from(at.y)));
        match wheel(event.delta, event.modifiers.shift) {
            Wheel::Zoom(f) => self.change_camera(cx, |c| c.zoom_about(cursor, f)),
            Wheel::Pan(dx, dy) => self.change_camera(cx, |c| c.pan(dx, dy)),
        }
    }
}

impl Render for BoardEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("board")
            .test_support()
            .track_focus(&self.focus)
            .key_context("BoardEditor")
            .size_full()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::press))
            .on_mouse_move(cx.listener(|this, e, _, cx| this.drag_to(e, cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.drag = None),
            )
            .on_scroll(cx.listener(|this, e, _, cx| this.scroll(e, cx)))
            .on_action(cx.listener(|this, _: &FitBoard, _, cx| this.fit(cx)))
            .child(BoardElement::new(
                Rc::clone(&self.opened),
                Rc::clone(&self.view),
            ))
    }
}
