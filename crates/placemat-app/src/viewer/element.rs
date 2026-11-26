//! The board canvas: one element that culls the scene once per frame, paints the visible
//! elements hand-drawn and publishes an accessibility leaf for each of them.

use std::rc::Rc;

use gpui_kit::accesskit::{self, Role};
use gpui_kit::{
    A11ySubtreeBuilder, App, Bounds, ContentMask, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, LayoutId, Path, PathBuilder, Pixels, Point, Rgba, ShapedLine, SharedString,
    Style, TextAlign, TextRun, Window, fill, font, point, px, relative, size,
};
use placemat_core::board::{Align, Board, Element as BoardItem, Fill, Kind, Stroke};
use placemat_core::scene::{
    Camera, Detail, Label, Op, OpSet, Rect, Scene, SetKind, Visible, dash_array, label_px,
};

use crate::viewer::budget::{Cost, Grant, allocate, label_cost, path_cost};
use crate::viewer::editor::ViewState;
use crate::viewer::frame::{self, Item};
use crate::viewer::measure::LINE_HEIGHT_EM;
use crate::viewer::open::OpenedBoard;
use crate::viewer::paint;

/// The side of the square a `Dot`-detail element paints, in logical pixels.
const DOT_PX: f32 = 2.0;

/// The canvas that paints an opened board through the shared camera.
#[non_exhaustive]
pub struct BoardElement {
    opened: Rc<OpenedBoard>,
    view: Rc<ViewState>,
}

impl BoardElement {
    /// A canvas over `opened`, reading and fitting the camera in `view`.
    pub(crate) fn new(opened: Rc<OpenedBoard>, view: Rc<ViewState>) -> Self {
        Self { opened, view }
    }
}

/// One thing to paint, in the board's paint order.
enum Draw {
    Path(Path, Rgba),
    Dot(Bounds<Pixels>, Rgba),
    Text(Box<TextLine>),
}

/// One shaped label line and where it sits; `align` places it within `width`.
struct TextLine {
    line: ShapedLine,
    origin: Point<Pixels>,
    line_height: Pixels,
    align: TextAlign,
    width: Pixels,
}

/// What one prepaint built: the visible items for the leaves and everything to paint.
#[non_exhaustive]
pub struct Frame {
    items: Vec<Item>,
    canvas: Rect,
    scale: f64,
    draws: Vec<Draw>,
}

impl Element for BoardElement {
    type RequestLayoutState = ();
    type PrepaintState = Frame;

    fn id(&self) -> Option<ElementId> {
        Some("canvas".into())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = Style {
            size: size(relative(1.).into(), relative(1.).into()),
            ..Style::default()
        };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Frame {
        // The first frame to learn the canvas size fits the board to it; later frames keep
        // whatever camera the editor holds.
        self.view.canvas.set(Some(bounds));
        let viewport = (
            f64::from(f32::from(bounds.size.width)),
            f64::from(f32::from(bounds.size.height)),
        );
        let camera = self.view.camera.get().unwrap_or_else(|| {
            let fitted = self
                .opened
                .scene
                .bounds()
                .map_or_else(Camera::default, |b| Camera::fit(b, viewport));
            self.view.camera.set(Some(fitted));
            fitted
        });
        let corner = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
        let board = &self.opened.loaded.board;
        let scale = f64::from(window.scale_factor());
        let planned = plan(&self.opened.scene, board, camera, viewport, scale);
        let hit = planned.iter().any(|p| !p.whole());
        if hit != self.view.budget_hit.replace(hit) && cfg!(debug_assertions) {
            let state = if hit { "cuts" } else { "no longer cuts" };
            eprintln!("placemat: the frame budget {state} detail or labels");
        }
        let mut items = Vec::with_capacity(planned.len());
        let mut draws = Vec::new();
        for p in planned {
            if let Some(e) = board.get(&p.item.id) {
                build(&mut draws, &p, e, camera, corner, window);
            }
            items.push(p.item);
        }
        Frame {
            items,
            canvas: rect_of(bounds),
            scale,
            draws,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        frame: &mut Frame,
        window: &mut Window,
        cx: &mut App,
    ) {
        let draws = std::mem::take(&mut frame.draws);
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for draw in draws {
                match draw {
                    Draw::Path(path, colour) => window.paint_path(path, colour),
                    Draw::Dot(at, colour) => window.paint_quad(fill(at, colour)),
                    Draw::Text(text) => {
                        let TextLine {
                            line,
                            origin,
                            line_height,
                            align,
                            width,
                        } = *text;
                        // A line that fails to paint leaves a gap; the frame goes on.
                        line.paint(origin, line_height, align, Some(width), window, cx)
                            .ok();
                    }
                }
            }
        });
    }

    fn a11y_role(&self) -> Option<Role> {
        Some(Role::Canvas)
    }

    fn a11y_synthetic_children(&mut self, frame: &mut Frame, builder: &mut A11ySubtreeBuilder) {
        for leaf in frame::leaves(
            &frame.items,
            &self.opened.loaded.board,
            frame.canvas,
            frame.scale,
        ) {
            let mut node = accesskit::Node::new(leaf.role);
            node.set_author_id(leaf.author.clone());
            node.set_label(leaf.name);
            node.set_bounds(leaf.bounds);
            let id = builder.synthetic_node_id(&leaf.author);
            builder.push_child(id, node);
        }
    }
}

/// A visible element with the detail and label the frame budget granted it.
struct Planned<'a> {
    v: Visible<'a>,
    item: Item,
    cost: Cost,
    grant: Grant,
}

impl Planned<'_> {
    /// Whether the budget left the item as its size on screen would draw it.
    fn whole(&self) -> bool {
        self.grant.detail == self.cost.natural && self.grant.label == (self.cost.glyphs > 0)
    }
}

/// Culls the scene through `camera`, prices each visible element and grants it what the
/// frame budget allows, in paint order.
fn plan<'a>(
    scene: &'a Scene,
    board: &Board,
    camera: Camera,
    viewport: (f64, f64),
    scale: f64,
) -> Vec<Planned<'a>> {
    let mut seen: Vec<(Visible<'a>, Item, Cost)> = scene
        .visible(camera.view(viewport))
        .map(|v| {
            let item = frame::item(&v, camera);
            let cost = cost_of(&v, item.detail, board, camera, scale);
            (v, item, cost)
        })
        .collect();
    let costs: Vec<Cost> = seen.iter().map(|s| s.2).collect();
    seen.drain(..)
        .zip(allocate(&costs))
        .map(|((v, item, cost), grant)| Planned {
            v,
            item,
            cost,
            grant,
        })
        .collect()
}

/// The path bytes `v` costs at Full and Simple detail and the glyphs of its label, if the
/// label is large enough to paint at this zoom.
fn cost_of(v: &Visible<'_>, natural: Detail, board: &Board, camera: Camera, scale: f64) -> Cost {
    let Some(e) = board.get(v.id) else {
        return Cost::new(natural, 0, 0, 0);
    };
    let bytes = |detail| -> u64 {
        paint::painted(v.geometry, detail)
            .map(|set| {
                let s = set_shape(set, &e.stroke, v.origin, camera);
                path_cost(s.ops, s.dashes, s.len * scale as f32)
            })
            .fold(0, u64::saturating_add)
    };
    let glyphs = v
        .geometry
        .label
        .as_ref()
        .filter(|l| label_px(l.world_size, camera.zoom).is_some())
        .map_or(0, |l| label_cost(&l.lines));
    Cost::new(natural, bytes(Detail::Full), bytes(Detail::Simple), glyphs)
}

/// The size of one op set on screen: its ops, its dashes and its length in logical pixels.
struct SetShape {
    ops: usize,
    dashes: f32,
    len: f32,
    dash: Option<[f32; 2]>,
}

fn set_shape(set: &OpSet, stroke: &Stroke, origin: (f64, f64), camera: Camera) -> SetShape {
    let len = paint::screen_len(&set.ops, origin, camera);
    let dash = (set.kind == SetKind::Stroke)
        .then(|| {
            dash_array(
                stroke.dash,
                paint::stroke_px(stroke.width, camera.zoom),
                len,
            )
        })
        .flatten();
    SetShape {
        ops: set.ops.len(),
        dashes: dash.map_or(0.0, |[on, off]| len / (on + off)),
        len,
        dash,
    }
}

/// Appends what `e` paints at its granted detail: its op sets and, if granted, its label, or
/// a dot.
fn build(
    draws: &mut Vec<Draw>,
    p: &Planned<'_>,
    e: &BoardItem,
    camera: Camera,
    corner: (f32, f32),
    window: &mut Window,
) {
    let (v, item) = (&p.v, &p.item);
    let stroke = e.stroke;
    if matches!(p.grant.detail, Detail::Dot) {
        let (x, y) = (
            corner.0 + ((item.screen.x0 + item.screen.x1) / 2.0) as f32,
            corner.1 + ((item.screen.y0 + item.screen.y1) / 2.0) as f32,
        );
        let at = Bounds::new(
            point(px(x - DOT_PX / 2.0), px(y - DOT_PX / 2.0)),
            size(px(DOT_PX), px(DOT_PX)),
        );
        draws.push(Draw::Dot(at, paint::rgba(stroke.colour)));
        return;
    }
    let fill_colour = match fill_of(e) {
        Fill::Colour { colour, .. } => Some(paint::rgba(colour)),
        Fill::None | _ => None,
    };
    for set in paint::painted(v.geometry, p.grant.detail) {
        let colour = match set.kind {
            SetKind::Fill | SetKind::Sketch => fill_colour,
            SetKind::Stroke | _ => Some(paint::rgba(stroke.colour)),
        };
        let Some(colour) = colour else { continue };
        if let Some(path) = path_of(set, &stroke, v.origin, camera, corner) {
            draws.push(Draw::Path(path, colour));
        }
    }
    if let Some(label) = v.geometry.label.as_ref().filter(|_| p.grant.label) {
        let align = match &e.kind {
            Kind::Text(t) => text_align(t.font.align),
            Kind::Rect(_)
            | Kind::Ellipse(_)
            | Kind::Diamond(_)
            | Kind::Line(_)
            | Kind::Arrow(_)
            | Kind::Freedraw(_)
            | _ => TextAlign::Center,
        };
        label_lines(
            draws, label, align, &stroke, v.origin, camera, corner, window,
        );
    }
}

/// Builds one op set as a canvas path in window pixels; None when the builder rejects it.
fn path_of(
    set: &OpSet,
    stroke: &Stroke,
    origin: (f64, f64),
    camera: Camera,
    corner: (f32, f32),
) -> Option<Path> {
    let zoom = camera.zoom;
    let mut b = match set.kind {
        SetKind::Fill => PathBuilder::fill(),
        SetKind::Sketch => PathBuilder::stroke(px(paint::sketch_px(stroke.width, zoom))),
        SetKind::Stroke | _ => PathBuilder::stroke(px(paint::stroke_px(stroke.width, zoom))),
    };
    if let Some([on, off]) = set_shape(set, stroke, origin, camera).dash {
        b = b.dash_array(&[px(on), px(off)]);
    }
    // paint_path ignores the element's offset, so every point carries the canvas corner.
    let at = |x: f32, y: f32| {
        let (sx, sy) = paint::to_px((x, y), origin, camera);
        point(px(corner.0 + sx), px(corner.1 + sy))
    };
    for op in &set.ops {
        match *op {
            Op::Move(x, y) => b.move_to(at(x, y)),
            Op::Line(x, y) => b.line_to(at(x, y)),
            Op::Cubic([ax, ay, bx, by, x, y]) => {
                b.cubic_bezier_to(at(x, y), at(ax, ay), at(bx, by))
            }
            _ => {}
        }
    }
    b.build().ok()
}

/// Shapes each label line in Inter at the label's screen size, aligned within the widest line;
/// a label too small to read adds nothing.
#[expect(
    clippy::too_many_arguments,
    reason = "one call site threading the frame's state"
)]
fn label_lines(
    draws: &mut Vec<Draw>,
    label: &Label,
    align: TextAlign,
    stroke: &Stroke,
    origin: (f64, f64),
    camera: Camera,
    corner: (f32, f32),
    window: &mut Window,
) {
    let Some(size_px) = label_px(label.world_size, camera.zoom) else {
        return;
    };
    let colour = Hsla::from(paint::rgba(stroke.colour));
    let line_height = size_px * LINE_HEIGHT_EM;
    let (x, y) = paint::to_px(label.at, origin, camera);
    let shaped: Vec<ShapedLine> = label
        .lines
        .iter()
        .map(|line| {
            let run = TextRun {
                len: line.len(),
                font: font("Inter"),
                color: colour,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            window.text_system().shape_line(
                SharedString::from(line.clone()),
                px(size_px),
                &[run],
                None,
            )
        })
        .collect();
    let width = shaped
        .iter()
        .map(ShapedLine::width)
        .fold(px(0.), Pixels::max);
    for (i, line) in shaped.into_iter().enumerate() {
        let origin = point(px(corner.0 + x), px(corner.1 + y + i as f32 * line_height));
        draws.push(Draw::Text(Box::new(TextLine {
            line,
            origin,
            line_height: px(line_height),
            align,
            width,
        })));
    }
}

fn fill_of(e: &BoardItem) -> Fill {
    match &e.kind {
        Kind::Rect(b) | Kind::Ellipse(b) | Kind::Diamond(b) => b.fill,
        Kind::Line(_) | Kind::Arrow(_) | Kind::Text(_) | Kind::Freedraw(_) | _ => Fill::None,
    }
}

fn text_align(align: Align) -> TextAlign {
    match align {
        Align::Center => TextAlign::Center,
        Align::Right => TextAlign::Right,
        Align::Left | _ => TextAlign::Left,
    }
}

fn rect_of(b: Bounds<Pixels>) -> Rect {
    let (x, y) = (
        f64::from(f32::from(b.origin.x)),
        f64::from(f32::from(b.origin.y)),
    );
    Rect::new(
        x,
        y,
        x + f64::from(f32::from(b.size.width)),
        y + f64::from(f32::from(b.size.height)),
    )
}

#[cfg(test)]
#[allow(dead_code, clippy::unwrap_used, clippy::panic)]
#[path = "../../../placemat-core/tests/board/util.rs"]
mod board_util;

#[cfg(test)]
mod tests {
    use placemat_core::board::read_file;
    use placemat_core::scene::FixedAdvance;

    use super::board_util::{all_kinds_board, limits_board, stacked_hachure_board};
    use super::*;
    use crate::viewer::budget::{CURVES_PER_DASH, CURVES_PER_OP, GLYPHS_MAX, PATH_BYTES_MAX};

    const VIEWPORT: (f64, f64) = (1280.0, 800.0);
    const SCALE: f64 = 2.0;
    const MEASURE: FixedAdvance = FixedAdvance::new(0.6, LINE_HEIGHT_EM);

    fn fitted(scene: &Scene) -> Camera {
        scene
            .bounds()
            .map_or_else(Camera::default, |b| Camera::fit(b, VIEWPORT))
    }

    fn at_max_zoom(scene: &Scene) -> Camera {
        let mut c = fitted(scene);
        c.zoom_about(
            (VIEWPORT.0 / 2.0, VIEWPORT.1 / 2.0),
            Camera::MAX_ZOOM / c.zoom,
        );
        c
    }

    fn granted_bytes(p: &Planned<'_>) -> u64 {
        match p.grant.detail {
            Detail::Full => p.cost.full,
            Detail::Simple => p.cost.simple,
            Detail::Dot | _ => 0,
        }
    }

    fn grid(name: &str) -> Board {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../placemat-core/tests/fixtures/boards/perf")
            .join(name);
        read_file(&path).unwrap().board
    }

    #[test]
    fn the_limits_board_at_fit_fills_the_glyph_pool_and_keeps_every_leaf() {
        let board = limits_board();
        let scene = Scene::build(&board, &MEASURE);
        let planned = plan(&scene, &board, fitted(&scene), VIEWPORT, SCALE);
        let glyphs: usize = planned
            .iter()
            .filter(|p| p.grant.label)
            .map(|p| p.cost.glyphs)
            .sum();
        assert!(glyphs <= GLYPHS_MAX, "{glyphs} glyphs granted");
        assert!(GLYPHS_MAX - glyphs < 105, "only {glyphs} glyphs granted");
        let bytes = planned
            .iter()
            .map(granted_bytes)
            .fold(0, u64::saturating_add);
        assert!(bytes <= PATH_BYTES_MAX, "{bytes} path bytes granted");
        let items: Vec<Item> = planned.iter().map(|p| p.item.clone()).collect();
        let canvas = Rect::new(0.0, 0.0, VIEWPORT.0, VIEWPORT.1);
        let leaves = frame::leaves(&items, &board, canvas, SCALE);
        assert_eq!(
            leaves.len(),
            items.len(),
            "an unpainted item still has its leaf"
        );
    }

    #[test]
    fn the_perf_grids_at_fit_paint_whole() {
        for name in ["grid-1000.board", "grid-10000.board"] {
            let board = grid(name);
            let scene = Scene::build(&board, &MEASURE);
            for p in plan(&scene, &board, fitted(&scene), VIEWPORT, SCALE) {
                assert!(
                    p.whole(),
                    "{name} {}: {:?} for {:?}",
                    p.item.id,
                    p.grant,
                    p.cost
                );
            }
        }
    }

    /// The curve-bound breaches of every set an item would paint at its natural detail, and
    /// the most curves seen per op on a solid set and per op on a dashed one.
    fn curve_counts(name: &str, board: &Board, camera: Camera) -> (Vec<String>, f64, f64) {
        let scene = Scene::build(board, &MEASURE);
        let (mut breaches, mut solid, mut dashed) = (Vec::new(), 0.0_f64, 0.0_f64);
        for p in plan(&scene, board, camera, VIEWPORT, SCALE) {
            let Some(e) = board.get(&p.item.id) else {
                continue;
            };
            for set in paint::painted(p.v.geometry, p.cost.natural) {
                let Some(path) = path_of(set, &e.stroke, p.v.origin, camera, (0.0, 0.0)) else {
                    continue;
                };
                let curves = format!("{path:?}").matches("PathCurve {").count() as u64;
                let s = set_shape(set, &e.stroke, p.v.origin, camera);
                let ops = s.ops as u64;
                let ratio = curves as f64 / ops.max(1) as f64;
                if s.dash.is_some() {
                    dashed = dashed.max(ratio);
                } else {
                    solid = solid.max(ratio);
                }
                // Flattening adds curves along the length, which the cost charges per tile.
                let dashes = (CURVES_PER_DASH as f32 * s.dashes).ceil() as u64;
                let tiles = (s.len * SCALE as f32 / 16.0).ceil() as u64;
                if curves > CURVES_PER_OP * ops + dashes + tiles {
                    breaches.push(format!(
                        "{name} {} {:?}: {curves} curves from {ops} ops, {} dashes, {} device px",
                        p.item.id,
                        set.kind,
                        s.dashes,
                        s.len * SCALE as f32
                    ));
                }
            }
        }
        (breaches, solid, dashed)
    }

    #[test]
    fn stroked_sets_stay_within_the_curve_bound() {
        let boards = [
            ("limits", limits_board()),
            ("all kinds", all_kinds_board()),
            ("stacked hachure", stacked_hachure_board()),
        ];
        let mut breaches = Vec::new();
        for (name, board) in &boards {
            let scene = Scene::build(board, &MEASURE);
            for (at, camera) in [("fit", fitted(&scene)), ("zoom 30", at_max_zoom(&scene))] {
                let (found, solid, dashed) =
                    curve_counts(&format!("{name} at {at}"), board, camera);
                println!(
                    "PERF curves {name} at {at}: at most {solid:.2} per op solid, {dashed:.2} \
                     dashed, {} breaches",
                    found.len()
                );
                breaches.extend(found);
            }
        }
        assert!(
            breaches.is_empty(),
            "{} breaches: {breaches:#?}",
            breaches.len()
        );
    }
}
