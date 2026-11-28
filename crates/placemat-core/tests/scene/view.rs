//! The camera transform and clamps, and the detail, label and dash rules at their thresholds.

use placemat_core::board::Dash;
use placemat_core::scene::{Camera, Detail, Rect, dash_array, detail, label_px};

fn close(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
}

#[test]
fn screen_and_world_round_trip() {
    let c = Camera::new((100.0, 50.0), 2.0);
    assert_eq!(c.to_screen((110.0, 60.0)), (20.0, 20.0));
    assert_eq!(c.to_world((20.0, 20.0)), (110.0, 60.0));
    assert_eq!(c.view((800.0, 600.0)), Rect::new(100.0, 50.0, 500.0, 350.0));
    assert_eq!(Camera::default(), Camera::new((0.0, 0.0), 1.0));
}

#[test]
fn new_clamps_the_zoom() {
    assert_eq!(Camera::new((0.0, 0.0), 100.0).zoom, Camera::MAX_ZOOM);
    assert_eq!(Camera::new((0.0, 0.0), 0.0).zoom, Camera::MIN_ZOOM);
    assert_eq!(Camera::new((0.0, 0.0), f64::NAN).zoom, 1.0);
}

#[test]
fn pan_moves_the_content_with_the_pointer() {
    let mut c = Camera::new((100.0, 50.0), 2.0);
    c.pan(10.0, -20.0);
    assert_eq!(c.origin, (95.0, 60.0));
    c.pan(f64::NAN, 1.0);
    assert_eq!(c.origin, (95.0, 60.0));
}

#[test]
fn zoom_about_keeps_the_point_under_the_cursor_and_clamps() {
    let mut c = Camera::default();
    c.zoom_about((200.0, 100.0), 2.0);
    assert_eq!((c.origin, c.zoom), ((100.0, 50.0), 2.0));
    let mut far = Camera::new((3.0, 4.0), 20.0);
    let under = far.to_world((640.0, 360.0));
    far.zoom_about((640.0, 360.0), 10.0);
    assert_eq!(far.zoom, Camera::MAX_ZOOM);
    assert!(close(far.to_world((640.0, 360.0)), under), "{far:?}");
    let before = far;
    for bad in [0.0, -2.0, f64::NAN, f64::INFINITY] {
        far.zoom_about((1.0, 1.0), bad);
        assert_eq!(far, before, "factor {bad}");
    }
}

#[test]
fn fit_centres_the_content_at_the_smaller_ratio() {
    let c = Camera::fit(Rect::new(0.0, 0.0, 1000.0, 500.0), (800.0, 600.0));
    assert!(
        (c.zoom - 0.736).abs() < 1e-12
            && close(c.origin, (500.0 - 400.0 / 0.736, 250.0 - 300.0 / 0.736)),
        "{c:?}"
    );
    let flat = Camera::fit(Rect::new(0.0, 0.0, 1000.0, 0.0), (800.0, 600.0));
    assert!(
        (flat.zoom - 0.736).abs() < 1e-12
            && close(flat.origin, (500.0 - 400.0 / 0.736, -300.0 / 0.736)),
        "{flat:?}"
    );
    let point = Camera::fit(Rect::new(5.0, 5.0, 5.0, 5.0), (800.0, 600.0));
    assert_eq!((point.zoom, point.origin), (1.0, (-395.0, -295.0)));
    let huge = Camera::fit(Rect::new(0.0, 0.0, 33_554_432.0, 10.0), (800.0, 600.0));
    assert_eq!(huge.zoom, Camera::MIN_ZOOM);
}

#[test]
fn fit_leaves_the_margin_on_the_limiting_axis() {
    assert_eq!(Camera::FIT_MARGIN, 32.0);
    let wide = Camera::fit(Rect::new(0.0, 0.0, 1000.0, 500.0), (800.0, 600.0));
    assert!((wide.zoom - 0.736).abs() < 1e-9, "{wide:?}");
    let (left, top) = wide.to_screen((0.0, 0.0));
    let (right, _) = wide.to_screen((1000.0, 500.0));
    assert!((left - 32.0).abs() < 1e-9, "{left}");
    assert!((right - 768.0).abs() < 1e-9, "{right}");
    assert!((top - 116.0).abs() < 1e-9, "{top}");

    let tall = Camera::fit(Rect::new(0.0, 0.0, 100.0, 1000.0), (800.0, 600.0));
    assert!((tall.zoom - 0.536).abs() < 1e-9, "{tall:?}");
    assert!(
        (tall.to_screen((0.0, 0.0)).1 - 32.0).abs() < 1e-9,
        "{tall:?}"
    );
    assert!(
        (tall.to_screen((0.0, 1000.0)).1 - 568.0).abs() < 1e-9,
        "{tall:?}"
    );
}

#[test]
fn fit_shrinks_the_margin_to_a_quarter_of_a_small_viewport() {
    let c = Camera::fit(Rect::new(0.0, 0.0, 100.0, 100.0), (40.0, 40.0));
    assert!((c.zoom - 0.2).abs() < 1e-9, "{c:?}");
    assert!((c.to_screen((0.0, 0.0)).0 - 10.0).abs() < 1e-9, "{c:?}");
}

#[test]
fn fit_in_a_degenerate_viewport_centres_the_content_at_zoom_1() {
    for viewport in [
        (0.0, 600.0),
        (-5.0, 600.0),
        (f64::NAN, 600.0),
        (800.0, f64::INFINITY),
    ] {
        let c = Camera::fit(Rect::new(0.0, 0.0, 1000.0, 500.0), viewport);
        assert_eq!((c.zoom, c.origin), (1.0, (500.0, 250.0)), "{viewport:?}");
    }
}

#[test]
fn fit_of_tiny_content_clamps_to_max_zoom_and_centres() {
    let c = Camera::fit(Rect::new(0.0, 0.0, 1.0, 1.0), (800.0, 600.0));
    assert_eq!(c.zoom, Camera::MAX_ZOOM);
    assert!(close(c.to_screen((0.5, 0.5)), (400.0, 300.0)), "{c:?}");
}

#[test]
fn detail_switches_at_its_thresholds() {
    assert_eq!(detail(1.99), Detail::Dot);
    assert_eq!(detail(f32::NAN), Detail::Dot);
    assert_eq!(detail(2.0), Detail::Simple);
    assert_eq!(detail(23.99), Detail::Simple);
    assert_eq!(detail(24.0), Detail::Full);
}

#[test]
fn label_sizes_snap_to_quarter_octaves_between_4_and_256() {
    assert_eq!(label_px(3.99, 1.0), None);
    assert_eq!(label_px(f32::NAN, 1.0), None);
    assert_eq!(label_px(4.0, 1.0), Some(4.0));
    assert_eq!(label_px(16.0, 1.0), Some(16.0));
    assert_eq!(label_px(16.0, 0.25), Some(4.0));
    assert_eq!(label_px(20.0, 1.0), Some(2f32.powf(17.0 / 4.0)));
    assert_eq!(label_px(1000.0, 1.0), Some(256.0));
    let mut sizes: Vec<u32> = (0..3000)
        .filter_map(|i| label_px(20.0, 0.1 + f64::from(i) * 0.01))
        .map(f32::to_bits)
        .collect();
    sizes.dedup();
    assert!(sizes.len() <= 25, "{} label sizes", sizes.len());
}

#[test]
fn dash_patterns_scale_with_width_and_fall_back_to_solid() {
    assert_eq!(dash_array(Dash::Solid, 2.0, 100.0), None);
    assert_eq!(dash_array(Dash::Dashed, 2.0, 100.0), Some([8.0, 10.0]));
    assert_eq!(dash_array(Dash::Dotted, 2.0, 100.0), Some([1.5, 8.0]));
    assert_eq!(dash_array(Dash::Dashed, 0.5, 100.0), Some([4.0, 5.0]));
    assert_eq!(
        dash_array(Dash::Dashed, 1.0, 9.0 * 2000.0),
        Some([4.0, 5.0])
    );
    assert_eq!(dash_array(Dash::Dashed, 1.0, 9.0 * 2000.0 + 9.0), None);
    assert_eq!(dash_array(Dash::Dashed, 1.0, f32::NAN), None);
    assert_eq!(dash_array(Dash::Dotted, f32::INFINITY, 100.0), None);
}
