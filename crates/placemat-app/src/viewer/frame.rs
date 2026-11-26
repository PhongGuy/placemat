//! One frame's worth of board: the visible elements mapped onto the canvas, and the
//! accessibility leaf each of them publishes.

use gpui_kit::accesskit::{self, Role};
use placemat_core::board::{Board, Element, ElementId, Kind};
use placemat_core::scene::{Camera, Detail, Rect, Scene, Visible, detail};

/// A visible element placed on the canvas.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// The element's id.
    pub id: ElementId,
    /// The element's position in the board's paint order.
    pub rank: u32,
    /// The element's bounds in canvas-relative logical pixels.
    pub screen: Rect,
    /// How much of the element to draw at this zoom.
    pub detail: Detail,
}

/// `v` mapped through `camera`, its detail chosen by its larger side on screen.
pub fn item(v: &Visible<'_>, camera: Camera) -> Item {
    let (x0, y0) = camera.to_screen((v.bounds.x0, v.bounds.y0));
    let (x1, y1) = camera.to_screen((v.bounds.x1, v.bounds.y1));
    let screen = Rect::new(x0, y0, x1, y1);
    Item {
        id: v.id.clone(),
        rank: v.rank,
        screen,
        detail: detail(screen.width().max(screen.height()) as f32),
    }
}

/// Every element a `viewport`-sized canvas shows through `camera`, in paint order.
pub fn cull(scene: &Scene, camera: Camera, viewport: (f64, f64)) -> Vec<Item> {
    scene
        .visible(camera.view(viewport))
        .map(|v| item(&v, camera))
        .collect()
}

/// Names the canvas node `board-canvas` (author id) and `Board` (label). Session trees carry
/// only author ids, so this is how they find the canvas; `board` stays the editor div's alone.
pub fn canvas_info(node: &mut accesskit::Node) {
    node.set_author_id("board-canvas".to_owned());
    node.set_label("Board".to_owned());
}

/// The accessibility author id of the element `id`: `shape-<id>`.
pub fn leaf_id(id: &ElementId) -> String {
    format!("shape-{}", id.as_str())
}

/// The accessibility role for an element kind: boxes group, text labels, strokes are images.
pub fn leaf_role(kind: &Kind) -> Role {
    match kind {
        Kind::Rect(_) | Kind::Ellipse(_) | Kind::Diamond(_) => Role::Group,
        Kind::Text(_) => Role::Label,
        Kind::Line(_) | Kind::Arrow(_) | Kind::Freedraw(_) => Role::Image,
        _ => Role::Group,
    }
}

/// The longest leaf name, in chars.
const NAME_MAX_CHARS: usize = 256;

/// The first line of the element's label or content, cut to 256 chars; `<Kind> <id>` when it
/// has none or it is empty.
pub fn leaf_name(e: &Element) -> String {
    let (kind, text) = match &e.kind {
        Kind::Rect(b) => ("Rect", b.label.as_deref()),
        Kind::Ellipse(b) => ("Ellipse", b.label.as_deref()),
        Kind::Diamond(b) => ("Diamond", b.label.as_deref()),
        Kind::Line(_) => ("Line", None),
        Kind::Arrow(a) => ("Arrow", a.label.as_deref()),
        Kind::Text(t) => ("Text", Some(t.content.as_str())),
        Kind::Freedraw(_) => ("Freedraw", None),
        _ => ("Shape", None),
    };
    let first = text
        .and_then(|t| t.split('\n').next())
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .unwrap_or_default();
    if first.is_empty() {
        format!("{kind} {}", e.id.as_str())
    } else {
        first.chars().take(NAME_MAX_CHARS).collect()
    }
}

/// `screen` moved onto the window by the canvas corner, clipped to `canvas`, times `scale`;
/// None when nothing is left.
pub fn leaf_bounds(screen: Rect, canvas: Rect, scale: f64) -> Option<accesskit::Rect> {
    let x0 = (screen.x0 + canvas.x0).max(canvas.x0);
    let y0 = (screen.y0 + canvas.y0).max(canvas.y0);
    let x1 = (screen.x1 + canvas.x0).min(canvas.x1);
    let y1 = (screen.y1 + canvas.y0).min(canvas.y1);
    (x0 < x1 && y0 < y1).then_some(accesskit::Rect {
        x0: x0 * scale,
        y0: y0 * scale,
        x1: x1 * scale,
        y1: y1 * scale,
    })
}

/// One accessibility leaf, as the element pushes it.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct Leaf {
    /// The author id, `shape-<id>`.
    pub author: String,
    /// The role for the element's kind.
    pub role: Role,
    /// The element's name.
    pub name: String,
    /// Window bounds in device pixels.
    pub bounds: accesskit::Rect,
}

/// One leaf per item, in `items` order (paint order); an item missing from `board` or wholly
/// off `canvas` (window logical px) is skipped.
pub fn leaves(items: &[Item], board: &Board, canvas: Rect, scale: f64) -> Vec<Leaf> {
    items
        .iter()
        .filter_map(|item| {
            let e = board.get(&item.id)?;
            Some(Leaf {
                author: leaf_id(&item.id),
                role: leaf_role(&e.kind),
                name: leaf_name(e),
                bounds: leaf_bounds(item.screen, canvas, scale)?,
            })
        })
        .collect()
}
