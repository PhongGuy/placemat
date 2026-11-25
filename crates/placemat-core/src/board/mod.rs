//! Placemat boards: the typed element model a `.board` file holds.
//!
//! Every value type checks its own format on construction, so a model built through this
//! API always describes a writable board.

mod model;
mod zkey;

pub use model::{
    Align, Anchor, Arrow, ArrowEnd, Boxed, Colour, Dash, Element, ElementId, Fill, FillStyle, Font,
    FontFamily, Head, Heads, Kind, Path, Point, Seed, Size, Stroke, Text,
};
pub use zkey::ZKey;
