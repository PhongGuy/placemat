//! Board colours on a real canvas: stored colours in light, the dark canvas filter in dark.

mod common;

use common::{Pilot, area, decode, fixture, launch_with, leaves, near, rect, segment};
use serde_json::json;

const FILL: [u8; 3] = [0xa5, 0xd8, 0xff];
const FILL_DARK: [u8; 3] = [0x15, 0x41, 0x63];
const INK: [u8; 3] = [0x1e, 0x1e, 0x1e];
const INK_DARK: [u8; 3] = [0xd3, 0xd3, 0xd3];
/// The solid fill is one flat polygon, so its pixels match to within rounding.
const FLAT_TOL: u8 = 2;
/// Glyph stems are solid in their cores; this keeps most antialiased edge pixels out.
const INK_TOL: u8 = 8;

/// The part of box `b` starting at fractions `(fx, fy)` of its size, `f` of its size across.
fn part(b: (f64, f64, f64, f64), fx: f64, fy: f64, f: f64) -> (f64, f64, f64, f64) {
    (b.0 + b.2 * fx, b.1 + b.3 * fy, b.2 * f, b.3 * f)
}

/// Opens `dark/fill.board` headless in `appearance` and checks the fill and the label ink
/// are `fill` and `ink`, and that the other appearance's colours are absent.
async fn colours_show(appearance: &str, fill: [u8; 3], ink: [u8; 3], absent: [[u8; 3]; 2]) {
    let pilot = Pilot::start().await;
    let dir = tempfile::TempDir::new().unwrap();
    let mut launch = launch_with("headless", &[&fixture("dark/fill.board")]);
    launch["appearance"] = json!(appearance);
    pilot.ok("launch", launch).await;
    let tree = pilot
        .ok("tree", json!({"wait_for": {"id": ["board"]}}))
        .await;
    let found = leaves(&tree);
    let shape = found
        .iter()
        .find(|n| segment(n) == "shape-darkFil1")
        .map(|n| rect(n))
        .unwrap_or_else(|| panic!("no shape-darkFil1: {tree}"));
    let path = dir.path().join(format!("colours-{appearance}.png"));
    pilot.ok("screenshot", json!({"save_to": path})).await;
    let shot = decode(&path);

    // A tenth-size box a fifth in from the top-left corner: flat fill that neither the
    // stroke at the edge nor the centred one-word label can reach.
    let corner = part(shape, 0.2, 0.2, 0.1);
    let flat = near(&shot, corner, fill, FLAT_TOL);
    let all = area(&shot, corner);
    assert!(all > 0, "the corner box is on screen: {corner:?}");
    assert!(
        flat * 10 >= all * 9,
        "{appearance}: {flat} of {all} corner pixels are the fill {fill:02x?}"
    );
    assert_eq!(
        near(&shot, corner, absent[0], FLAT_TOL),
        0,
        "{appearance}: no {:02x?} in the fill",
        absent[0]
    );

    // The middle fifth holds the whole label and none of the stroke.
    let middle = part(shape, 0.4, 0.4, 0.2);
    let inked = near(&shot, middle, ink, INK_TOL);
    assert!(
        inked >= 200,
        "{appearance}: the label paints {inked} pixels of ink {ink:02x?}"
    );
    assert_eq!(
        near(&shot, middle, absent[1], INK_TOL),
        0,
        "{appearance}: no {:02x?} in the label",
        absent[1]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn light_shows_the_stored_fill_and_ink() {
    colours_show("light", FILL, INK, [FILL_DARK, INK_DARK]).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn dark_shows_the_filtered_fill_and_light_ink() {
    colours_show("dark", FILL_DARK, INK_DARK, [FILL, INK]).await;
}
