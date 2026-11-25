//! The value types: rectangles and the fixed-advance measurer.

use super::{FixedAdvance, Rect, TextMeasure};

#[test]
fn rect_new_sorts_each_axis() {
    let r = Rect::new(10.0, 5.0, -2.0, 7.0);
    assert_eq!((r.x0, r.y0, r.x1, r.y1), (-2.0, 5.0, 10.0, 7.0));
    assert_eq!((r.width(), r.height()), (12.0, 2.0));
}

#[test]
fn rects_that_touch_intersect_and_union_covers_both() {
    let a = Rect::new(0.0, 0.0, 10.0, 10.0);
    assert!(
        a.intersects(&Rect::new(10.0, 10.0, 20.0, 20.0)),
        "touching corners"
    );
    assert!(!a.intersects(&Rect::new(10.5, 0.0, 20.0, 10.0)), "apart");
    assert_eq!(
        a.union(&Rect::new(-5.0, 3.0, 4.0, 30.0)),
        Rect::new(-5.0, 0.0, 10.0, 30.0)
    );
}

#[test]
fn fixed_advance_counts_characters_not_bytes() {
    let m = FixedAdvance::new(0.5, 1.25);
    assert_eq!(m.line_width("é€a", 20.0), 3.0 * 0.5 * 20.0);
    assert_eq!(m.line_height(20.0), 1.25 * 20.0);
}
