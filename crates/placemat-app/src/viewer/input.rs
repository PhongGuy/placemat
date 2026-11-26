//! The wheel mapping: what one scroll event does to the camera.

use gpui_kit::{ScrollDelta, px};

/// The zoom factor of one wheel notch.
pub const NOTCH_ZOOM: f64 = 1.1;
/// The lines one wheel notch scrolls.
pub const LINES_PER_NOTCH: f32 = 3.0;
/// The pixels one scrolled line counts for.
pub const LINE_PX: f32 = 20.0;

/// What a wheel event does to the camera.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wheel {
    /// Scale the zoom by this factor about the cursor.
    Zoom(f64),
    /// Move the content by this screen-pixel delta.
    Pan(f64, f64),
}

/// Maps a scroll to a camera change: the vertical wheel zooms, one notch up by
/// `NOTCH_ZOOM`, and with Shift held the whole delta pans the content instead.
pub fn wheel(delta: ScrollDelta, shift: bool) -> Wheel {
    if shift {
        let d = delta.pixel_delta(px(LINE_PX));
        return Wheel::Pan(f64::from(f32::from(d.x)), f64::from(f32::from(d.y)));
    }
    // A pixel scroll counts a notch per notch's worth of lines, so both sources zoom alike.
    let notches = match delta {
        ScrollDelta::Lines(d) => d.y / LINES_PER_NOTCH,
        ScrollDelta::Pixels(d) => f32::from(d.y) / (LINES_PER_NOTCH * LINE_PX),
    };
    Wheel::Zoom(NOTCH_ZOOM.powf(f64::from(notches)))
}
