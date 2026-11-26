//! The board canvas: one element that culls the scene once per frame, paints the visible
//! elements hand-drawn and publishes an accessibility leaf for each of them.

use std::rc::Rc;

use gpui_kit::accesskit::{self, Role};
use gpui_kit::{
    A11ySubtreeBuilder, App, Bounds, ContentMask, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, LayoutId, LineJoin, Path, PathBuilder, PathStyle, Pixels, Point, Rgba,
    ShapedLine, SharedString, StrokeOptions, Style, TextAlign, TextRun, Window, fill, font, point,
    px, relative, size,
};
use placemat_core::board::{Align, Board, Element as BoardItem, Fill, Kind, Stroke};
use placemat_core::scene::{
    Camera, Detail, Label, Op, OpSet, Rect, Scene, SetKind, Visible, dash_array, label_px,
};

use crate::viewer::budget::{
    Cost, Grant, allocate, label_cost, path_cost, stroke_curves, stroke_l1, whole,
};
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
        let view = View {
            camera,
            viewport,
            scale,
        };
        let planned = plan(&self.opened.scene, board, view);
        let hit = planned.iter().any(|p| !whole(&p.cost, &p.grant));
        if hit != self.view.budget_hit.replace(hit) && cfg!(debug_assertions) {
            let state = if hit { "cuts" } else { "no longer cuts" };
            eprintln!("placemat: the frame budget {state} detail or labels");
        }
        let mut items = Vec::with_capacity(planned.len());
        let mut draws = Vec::new();
        for p in planned {
            if let Some(e) = board.get(&p.item.id) {
                build(&mut draws, &p, e, view, corner, window);
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

    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        frame::canvas_info(node);
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
    /// Each of `v.geometry.rough`'s sets priced, empty when the item cannot paint at Full.
    rough: Vec<Priced>,
    /// Each of `v.geometry.outline`'s sets priced, empty when the item paints as a dot.
    outline: Vec<Priced>,
}

impl Planned<'_> {
    /// The sets to paint at the granted detail with their dash patterns: Full all `rough`,
    /// Simple the `outline`'s Stroke sets, as `paint::painted` chooses them.
    fn sets(&self) -> Vec<(&OpSet, Option<[f32; 2]>)> {
        let g = self.v.geometry;
        match self.grant.detail {
            Detail::Full => g
                .rough
                .iter()
                .zip(&self.rough)
                .map(|(s, p)| (s, p.dash))
                .collect(),
            Detail::Simple => g
                .outline
                .iter()
                .zip(&self.outline)
                .filter(|(s, _)| s.kind == SetKind::Stroke)
                .map(|(s, p)| (s, p.dash))
                .collect(),
            Detail::Dot | _ => Vec::new(),
        }
    }
}

/// One op set's dash pattern and estimated instance bytes this frame.
#[derive(Clone, Copy, Debug)]
struct Priced {
    dash: Option<[f32; 2]>,
    bytes: u64,
}

/// Where the frame paints: the canvas as the camera sees it, and the device scale.
#[derive(Clone, Copy, Debug)]
struct View {
    camera: Camera,
    viewport: (f64, f64),
    scale: f64,
}

/// Culls the scene through the camera, prices each visible element and grants it what the
/// frame budget allows, in paint order.
fn plan<'a>(scene: &'a Scene, board: &Board, view: View) -> Vec<Planned<'a>> {
    let mut seen: Vec<Planned<'a>> = scene
        .visible(view.camera.view(view.viewport))
        .map(|v| {
            let item = frame::item(&v, view.camera);
            let natural = item.detail;
            let (rough, outline) = match board.get(v.id) {
                Some(e) => price_sets(&v, natural, &e.stroke, view),
                None => (Vec::new(), Vec::new()),
            };
            let sum = |sets: &[Priced]| sets.iter().map(|p| p.bytes).fold(0, u64::saturating_add);
            let simple: Vec<Priced> = v
                .geometry
                .outline
                .iter()
                .zip(&outline)
                .filter(|(s, _)| s.kind == SetKind::Stroke)
                .map(|(_, p)| *p)
                .collect();
            let glyphs = v
                .geometry
                .label
                .as_ref()
                .filter(|l| label_px(l.world_size, view.camera.zoom).is_some())
                .map_or(0, |l| label_cost(&l.lines));
            let cost = Cost::new(natural, sum(&rough), sum(&simple), glyphs);
            Planned {
                v,
                item,
                cost,
                grant: Grant::new(Detail::Dot, false),
                rough,
                outline,
            }
        })
        .collect();
    let costs: Vec<Cost> = seen.iter().map(|p| p.cost).collect();
    for (p, grant) in seen.iter_mut().zip(allocate(&costs)) {
        p.grant = grant;
    }
    seen
}

/// Prices the sets an item at `natural` detail could paint: `rough` for Full, `outline` for
/// Full or Simple; a dot prices nothing.
fn price_sets(
    v: &Visible<'_>,
    natural: Detail,
    stroke: &Stroke,
    view: View,
) -> (Vec<Priced>, Vec<Priced>) {
    let price = |set: &OpSet| price(set, stroke, v.origin, view);
    match natural {
        Detail::Full => (
            v.geometry.rough.iter().map(price).collect(),
            v.geometry.outline.iter().map(price).collect(),
        ),
        Detail::Simple => (Vec::new(), v.geometry.outline.iter().map(price).collect()),
        Detail::Dot | _ => (Vec::new(), Vec::new()),
    }
}

/// The set's curve bound, device box and L1 length, and its dash pattern, from one walk.
struct Shape {
    curves: u64,
    bounds: Rect,
    l1: f64,
    dash: Option<[f32; 2]>,
}

fn shape(set: &OpSet, stroke: &Stroke, origin: (f64, f64), view: View) -> Shape {
    let zoom = view.camera.zoom;
    let w = paint::walk(&set.ops, origin, view.camera, view.scale);
    let len = w.len as f32;
    let (width, dash) = match set.kind {
        SetKind::Fill => (0.0, None),
        SetKind::Sketch => (paint::sketch_px(stroke.width, zoom), None),
        SetKind::Stroke | _ => {
            let width = paint::stroke_px(stroke.width, zoom);
            (width, dash_array(stroke.dash, width, len))
        }
    };
    let dashes = dash.map_or(0.0, |[on, off]| len / (on + off));
    let s = view.scale;
    let curves = match set.kind {
        SetKind::Fill => w.fill_curves,
        SetKind::Sketch | SetKind::Stroke | _ => stroke_curves(w.segments, w.subpaths, dashes),
    };
    // A stroke reaches half its width past the control points.
    let half = f64::from(width) / 2.0;
    let b = w.bounds.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
    let bounds = Rect::new(
        (b.x0 - half) * s,
        (b.y0 - half) * s,
        (b.x1 + half) * s,
        (b.y1 + half) * s,
    );
    Shape {
        curves,
        bounds,
        l1: match set.kind {
            SetKind::Fill => w.l1 * s,
            SetKind::Sketch | SetKind::Stroke | _ => stroke_l1(
                w.l1 * s,
                f64::from(width) * s,
                w.segments,
                w.subpaths,
                dashes,
            ),
        },
        dash,
    }
}

fn price(set: &OpSet, stroke: &Stroke, origin: (f64, f64), view: View) -> Priced {
    let sh = shape(set, stroke, origin, view);
    let clip = Rect::new(
        0.0,
        0.0,
        view.viewport.0 * view.scale,
        view.viewport.1 * view.scale,
    );
    Priced {
        dash: sh.dash,
        bytes: path_cost(sh.curves, sh.bounds, sh.l1, clip),
    }
}

/// Appends what `e` paints at its granted detail: its op sets and, if granted, its label, or
/// a dot.
fn build(
    draws: &mut Vec<Draw>,
    p: &Planned<'_>,
    e: &BoardItem,
    view: View,
    corner: (f32, f32),
    window: &mut Window,
) {
    let (v, item, camera, scale) = (&p.v, &p.item, view.camera, view.scale);
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
    for (set, dash) in p.sets() {
        let colour = match set.kind {
            SetKind::Fill | SetKind::Sketch => fill_colour,
            SetKind::Stroke | _ => Some(paint::rgba(stroke.colour)),
        };
        let Some(colour) = colour else { continue };
        if let Some(path) = path_of(set, &stroke, dash, v.origin, camera, corner, scale) {
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
    dash: Option<[f32; 2]>,
    origin: (f64, f64),
    camera: Camera,
    corner: (f32, f32),
    scale: f64,
) -> Option<Path> {
    let zoom = camera.zoom;
    // Strokes join with bevels and arrive as lines: tiny-skia's curve offsetting has no
    // bound on the curves it makes, while a line's offset and bevel are always five.
    let stroked = |width: f32| {
        PathBuilder::fill().with_style(PathStyle::Stroke(StrokeOptions {
            line_width: width,
            line_join: LineJoin::Bevel,
            ..StrokeOptions::default()
        }))
    };
    let mut b = match set.kind {
        SetKind::Fill => PathBuilder::fill(),
        SetKind::Sketch => stroked(paint::sketch_px(stroke.width, zoom)),
        SetKind::Stroke | _ => stroked(paint::stroke_px(stroke.width, zoom)),
    };
    if let Some([on, off]) = dash {
        b = b.dash_array(&[px(on), px(off)]);
    }
    // paint_path ignores the element's offset, so every point carries the canvas corner.
    let local = |x: f32, y: f32| paint::to_px((x, y), origin, camera);
    let at = |(x, y): (f32, f32)| point(px(corner.0 + x), px(corner.1 + y));
    let mut current = (0.0_f32, 0.0_f32);
    for op in &set.ops {
        match *op {
            Op::Move(x, y) => {
                current = local(x, y);
                b.move_to(at(current));
            }
            Op::Line(x, y) => {
                current = local(x, y);
                b.line_to(at(current));
            }
            Op::Cubic([ax, ay, bx, by, x, y]) => {
                let p = [current, local(ax, ay), local(bx, by), local(x, y)];
                if set.kind == SetKind::Fill {
                    b.cubic_bezier_to(at(p[3]), at(p[1]), at(p[2]));
                } else {
                    let n = paint::cubic_segments(p, scale);
                    for i in 1..n {
                        b.line_to(at(cubic_at(p, f64::from(i) / f64::from(n))));
                    }
                    b.line_to(at(p[3]));
                }
                current = p[3];
            }
            _ => {}
        }
    }
    b.build().ok()
}

/// The point at `t` along the cubic with control points `p`.
fn cubic_at(p: [(f32, f32); 4], t: f64) -> (f32, f32) {
    let u = 1.0 - t;
    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    let axis = |f: fn((f32, f32)) -> f32| -> f32 {
        p.iter()
            .zip(w)
            .map(|(q, k)| f64::from(f(*q)) * k)
            .sum::<f64>() as f32
    };
    (axis(|q| q.0), axis(|q| q.1))
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
    use placemat_core::board::{Boxed, FillStyle, Point, Size, read_file};
    use placemat_core::scene::FixedAdvance;

    use super::board_util::{
        all_kinds_board, el, extremes_board, limits_board, stacked_hachure_board,
        stacked_hachure_boxes,
    };
    use super::*;
    use crate::viewer::budget::{GLYPHS_MAX, PATH_BYTES_MAX};

    const VIEWPORT: (f64, f64) = (1280.0, 800.0);
    const SCALE: f64 = 2.0;
    const MEASURE: FixedAdvance = FixedAdvance::new(0.6, LINE_HEIGHT_EM);

    fn fitted(scene: &Scene) -> View {
        let camera = scene
            .bounds()
            .map_or_else(Camera::default, |b| Camera::fit(b, VIEWPORT));
        View {
            camera,
            viewport: VIEWPORT,
            scale: SCALE,
        }
    }

    fn at_max_zoom(scene: &Scene) -> View {
        let mut view = fitted(scene);
        let factor = Camera::MAX_ZOOM / view.camera.zoom;
        view.camera
            .zoom_about((VIEWPORT.0 / 2.0, VIEWPORT.1 / 2.0), factor);
        view
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

    /// One hachured, dashed box `side` units square, centred on the origin.
    fn hachured_box(side: i32) -> Board {
        let mut body = Boxed::new(Point::new(-side / 2, -side / 2), Size::new(side, side));
        body.fill = Fill::Colour {
            colour: placemat_core::board::Colour::rgb(0xa5, 0xd8, 0xff),
            style: FillStyle::Hachure,
        };
        let mut e = el("box", 1, "a1", Kind::Rect(body));
        e.stroke.dash = placemat_core::board::Dash::Dashed;
        let mut b = Board::new();
        b.insert(e).unwrap();
        b
    }

    #[test]
    fn the_limits_board_at_fit_fills_the_glyph_pool_and_keeps_every_leaf() {
        let board = limits_board();
        let scene = Scene::build(&board, &MEASURE);
        let planned = plan(&scene, &board, fitted(&scene));
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
            for p in plan(&scene, &board, fitted(&scene)) {
                assert!(
                    whole(&p.cost, &p.grant),
                    "{name} {}: {:?} for {:?}",
                    p.item.id,
                    p.grant,
                    p.cost
                );
            }
        }
    }

    #[test]
    fn a_hachured_box_keeps_its_fill_at_full_zoom() {
        let board = hachured_box(1000);
        let scene = Scene::build(&board, &MEASURE);
        let planned = plan(&scene, &board, at_max_zoom(&scene));
        let p = &planned[0];
        assert_eq!(p.grant.detail, Detail::Full, "{:?}", p.cost);
        assert!(
            p.cost.full <= PATH_BYTES_MAX,
            "estimated {} bytes",
            p.cost.full
        );
    }

    #[test]
    fn the_largest_box_at_fit_drops_to_its_outline_when_full_overflows_the_pool() {
        let board = hachured_box(1 << 24);
        let scene = Scene::build(&board, &MEASURE);
        let planned = plan(&scene, &board, fitted(&scene));
        let p = &planned[0];
        assert_eq!(p.grant.detail, Detail::Simple, "{:?}", p.cost);
        assert!(
            p.cost.full > PATH_BYTES_MAX,
            "Full would have fit: {:?}",
            p.cost
        );
    }

    /// Asserts the allocation rule: the pools hold everything granted, and an item was cut
    /// only when the next detail up cost more than was left at its turn, topmost first.
    fn assert_the_rule(name: &str, planned: &[Planned<'_>]) {
        let glyphs: usize = planned
            .iter()
            .filter(|p| p.grant.label)
            .map(|p| p.cost.glyphs)
            .sum();
        assert!(glyphs <= GLYPHS_MAX, "{name}: {glyphs} glyphs granted");
        let mut left = PATH_BYTES_MAX;
        for p in planned.iter().rev() {
            let up = match (p.cost.natural, p.grant.detail) {
                (Detail::Full, Detail::Simple) => Some(p.cost.full),
                (Detail::Full | Detail::Simple, Detail::Dot) => Some(p.cost.simple),
                _ => None,
            };
            if let Some(up) = up {
                assert!(
                    up > left,
                    "{name} {}: cut with {left} left: {:?}",
                    p.item.id,
                    p.cost
                );
            }
            let spent = granted_bytes(p);
            assert!(spent <= left, "{name} {} overdrew the pool", p.item.id);
            left -= spent;
        }
    }

    #[test]
    fn the_limits_board_at_full_zoom_degrades_only_what_the_pool_cannot_hold() {
        let board = limits_board();
        let scene = Scene::build(&board, &MEASURE);
        let planned = plan(&scene, &board, at_max_zoom(&scene));
        let top = planned.last().unwrap();
        assert_eq!(
            top.grant.detail, top.cost.natural,
            "the topmost item is whole"
        );
        assert_the_rule("limits at zoom 30", &planned);
    }

    #[test]
    fn the_stacked_boxes_at_fit_follow_the_rule() {
        let board = stacked_hachure_board();
        let scene = Scene::build(&board, &MEASURE);
        assert_the_rule("stacked at fit", &plan(&scene, &board, fitted(&scene)));
    }

    #[test]
    fn a_hundred_stacked_small_boxes_at_full_zoom_nearly_fill_the_pool() {
        let board = stacked_hachure_boxes(1000);
        let scene = Scene::build(&board, &MEASURE);
        let planned = plan(&scene, &board, at_max_zoom(&scene));
        assert_the_rule("small stacked at zoom 30", &planned);
        let full = planned
            .iter()
            .filter(|p| p.grant.detail == Detail::Full)
            .count();
        let bytes = planned
            .iter()
            .map(granted_bytes)
            .fold(0, u64::saturating_add);
        assert!(full >= 7, "only {full} boxes granted Full");
        assert!(bytes > 48 << 20, "only {bytes} bytes granted");
        println!("PERF estimate small stacked at zoom 30: {bytes} bytes, {full} Full");
    }

    /// The curve-bound breaches of every set an item would paint at its natural detail, and
    /// the most real curves seen against the bound, per set kind.
    fn curve_counts(name: &str, board: &Board, view: View) -> (Vec<String>, [f64; 2]) {
        let scene = Scene::build(board, &MEASURE);
        let (mut breaches, mut worst) = (Vec::new(), [0.0_f64; 2]);
        for p in plan(&scene, board, view) {
            let Some(e) = board.get(&p.item.id) else {
                continue;
            };
            for set in paint::painted(p.v.geometry, p.cost.natural) {
                let sh = shape(set, &e.stroke, p.v.origin, view);
                let built = path_of(
                    set,
                    &e.stroke,
                    sh.dash,
                    p.v.origin,
                    view.camera,
                    (0.0, 0.0),
                    view.scale,
                );
                let Some(path) = built else {
                    continue;
                };
                let curves = format!("{path:?}").matches("PathCurve {").count() as u64;
                let k = usize::from(set.kind != SetKind::Fill);
                worst[k] = worst[k].max(curves as f64 / sh.curves.max(1) as f64);
                if curves > sh.curves {
                    breaches.push(format!(
                        "{name} {} {:?}: {curves} curves over a bound of {} ({} ops, dash {:?})",
                        p.item.id,
                        set.kind,
                        sh.curves,
                        set.ops.len(),
                        sh.dash
                    ));
                }
            }
        }
        (breaches, worst)
    }

    #[test]
    fn built_sets_stay_within_their_curve_bound() {
        let boards = [
            ("limits", limits_board()),
            ("all kinds", all_kinds_board()),
            ("extremes", extremes_board()),
            ("stacked hachure", stacked_hachure_board()),
        ];
        let mut breaches = Vec::new();
        for (name, board) in &boards {
            let scene = Scene::build(board, &MEASURE);
            for (at, view) in [("fit", fitted(&scene)), ("zoom 30", at_max_zoom(&scene))] {
                let (found, [fill, stroke]) = curve_counts(&format!("{name} at {at}"), board, view);
                println!(
                    "PERF curves {name} at {at}: worst real/bound {fill:.3} fill, {stroke:.3} \
                     stroke, {} breaches",
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
