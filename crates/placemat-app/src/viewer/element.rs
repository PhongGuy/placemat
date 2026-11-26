//! The board canvas: one element that culls the scene once per frame, paints the visible
//! elements hand-drawn and publishes an accessibility leaf for each of them.

use std::rc::Rc;

use gpui_kit::accesskit::{self, Role};
use gpui_kit::{
    A11ySubtreeBuilder, App, Bounds, ContentMask, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, LayoutId, Path, PathBuilder, Pixels, Point, Rgba, ShapedLine, SharedString,
    Style, TextAlign, TextRun, Window, fill, font, point, px, relative, size,
};
use placemat_core::board::{Align, Element as BoardItem, Fill, Kind, Stroke};
use placemat_core::scene::{
    Camera, Detail, Label, Op, OpSet, Rect, SetKind, Visible, dash_array, label_px,
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
        let mut items = Vec::new();
        let mut draws = Vec::new();
        for v in self.opened.scene.visible(camera.view(viewport)) {
            let item = frame::item(&v, camera);
            if let Some(e) = board.get(&item.id) {
                build(&mut draws, &v, &item, e, camera, corner, window);
            }
            items.push(item);
        }
        Frame {
            items,
            canvas: rect_of(bounds),
            scale: f64::from(window.scale_factor()),
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

/// Appends what `e` paints at its item's detail: its op sets and label, or a dot.
fn build(
    draws: &mut Vec<Draw>,
    v: &Visible<'_>,
    item: &Item,
    e: &BoardItem,
    camera: Camera,
    corner: (f32, f32),
    window: &mut Window,
) {
    let stroke = e.stroke;
    if matches!(item.detail, Detail::Dot) {
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
    for set in paint::painted(v.geometry, item.detail) {
        let colour = match set.kind {
            SetKind::Fill | SetKind::Sketch => fill_colour,
            SetKind::Stroke | _ => Some(paint::rgba(stroke.colour)),
        };
        let Some(colour) = colour else { continue };
        if let Some(path) = path_of(set, &stroke, v.origin, camera, corner) {
            draws.push(Draw::Path(path, colour));
        }
    }
    if let Some(label) = &v.geometry.label {
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
    if set.kind == SetKind::Stroke {
        let len = paint::screen_len(&set.ops, origin, camera);
        if let Some([on, off]) = dash_array(stroke.dash, paint::stroke_px(stroke.width, zoom), len)
        {
            b = b.dash_array(&[px(on), px(off)]);
        }
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
