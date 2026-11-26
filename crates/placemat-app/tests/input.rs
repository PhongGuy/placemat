//! The wheel: zoom by notches, Shift to pan.

use gpui_kit::{ScrollDelta, point, px};
use placemat_app::viewer::input::{Wheel, wheel};

fn zoom(w: Wheel) -> f64 {
    match w {
        Wheel::Zoom(f) => f,
        other @ (Wheel::Pan(..) | _) => panic!("not a zoom: {other:?}"),
    }
}

#[test]
fn a_notch_up_zooms_in_by_a_tenth_and_down_zooms_out() {
    assert!((zoom(wheel(ScrollDelta::Lines(point(0.0, 3.0)), false)) - 1.1).abs() < 1e-9);
    assert!((zoom(wheel(ScrollDelta::Lines(point(0.0, -3.0)), false)) - 1.0 / 1.1).abs() < 1e-9);
    assert!((zoom(wheel(ScrollDelta::Lines(point(0.0, 6.0)), false)) - 1.21).abs() < 1e-9);
}

#[test]
fn pixel_scrolls_count_sixty_pixels_a_notch() {
    assert!((zoom(wheel(ScrollDelta::Pixels(point(px(0.0), px(60.0))), false)) - 1.1).abs() < 1e-9);
    assert!((zoom(wheel(ScrollDelta::Pixels(point(px(0.0), px(0.0))), false)) - 1.0).abs() < 1e-9);
}

#[test]
fn shift_pans_by_the_pixel_delta() {
    assert_eq!(
        wheel(ScrollDelta::Lines(point(-3.0, 0.0)), true),
        Wheel::Pan(-60.0, 0.0)
    );
    assert_eq!(
        wheel(ScrollDelta::Pixels(point(px(5.0), px(7.0))), true),
        Wheel::Pan(5.0, 7.0)
    );
}
