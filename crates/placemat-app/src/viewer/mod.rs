//! The board viewer: a board file opened off the UI thread and shown on a pan-and-zoom canvas
//! that paints each visible shape hand-drawn and publishes one accessibility leaf per shape.

use std::rc::Rc;

use gpui_kit::{App, KeyBinding, KeyBindingContextPredicate, actions};

pub mod budget;
pub mod editor;
pub mod element;
pub mod frame;
pub mod input;
pub mod measure;
pub mod notice;
pub mod open;
pub mod paint;

actions!(
    placemat,
    [
        /// Fits the whole board to the canvas again.
        FitBoard,
        /// Asks for a board file through the system dialog and opens it.
        OpenBoard
    ]
);

/// Binds the viewer's keys; the fit key reaches a board editor once a click has focused it,
/// the open key reaches the main view from anywhere inside it.
pub fn init(cx: &mut App) {
    // `secondary-` means Cmd or Ctrl only once the host OS is known, which `load` reads and
    // `KeyBinding::new` does not.
    let context = KeyBindingContextPredicate::parse("BoardEditor")
        .ok()
        .map(Rc::new);
    if let Ok(fit) = KeyBinding::load("secondary-0", Box::new(FitBoard), context, false, None, cx) {
        cx.bind_keys([fit]);
    }
    let context = KeyBindingContextPredicate::parse("MainView")
        .ok()
        .map(Rc::new);
    if let Ok(open) = KeyBinding::load("secondary-o", Box::new(OpenBoard), context, false, None, cx)
    {
        cx.bind_keys([open]);
    }
}
