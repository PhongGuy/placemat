//! The board viewer: opening a board file off the UI thread and measuring its text with GPUI.

use std::rc::Rc;

use gpui_kit::{App, KeyBinding, KeyBindingContextPredicate, actions};

pub mod editor;
pub mod element;
pub mod frame;
pub mod input;
pub mod measure;
pub mod open;
pub mod paint;

actions!(
    placemat,
    [
        /// Fits the whole board to the canvas again.
        FitBoard
    ]
);

/// Binds the viewer's keys; the fit key reaches a board editor once a click has focused it.
pub fn init(cx: &mut App) {
    // `secondary-` means Cmd or Ctrl only once the host OS is known, which `load` reads and
    // `KeyBinding::new` does not.
    let context = KeyBindingContextPredicate::parse("BoardEditor")
        .ok()
        .map(Rc::new);
    if let Ok(fit) = KeyBinding::load("secondary-0", Box::new(FitBoard), context, false, None, cx) {
        cx.bind_keys([fit]);
    }
}
