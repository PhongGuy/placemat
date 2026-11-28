//! Filled arrowheads on a real canvas: a dot or triangle head is solid ink, light and dark.

mod common;

use common::{Pilot, area, decode, fixture, launch_with, leaves, near, rect, segment};
use placemat_core::board::read_file;
use placemat_core::scene::{FixedAdvance, Scene};
use serde_json::json;

const INK: [u8; 3] = [0x1e, 0x1e, 0x1e];
const INK_DARK: [u8; 3] = [0xd3, 0xd3, 0xd3];
/// The head's core is one flat polygon, so its pixels match to within rounding.
const FLAT_TOL: u8 = 2;
/// The probe's side in world units.
const PROBE: f64 = 0.5;

/// Each arrow in `dark/heads.board` and a point inside its end head that neither the outline
/// nor the shaft reaches, local to the arrow's start; exact because the arrows have no roughness.
const HEADS: [(&str, (f64, f64)); 2] = [("darkDot1", (38.0, 2.0)), ("darkTri1", (34.0, 1.25))];

/// The world bounds of arrow `id` as (x, y, w, h), and its start.
fn placed(id: &str) -> ((f64, f64, f64, f64), (f64, f64)) {
    let board = read_file(&fixture("dark/heads.board")).unwrap().board;
    let scene = Scene::build(&board, &FixedAdvance::new(0.6, 1.25));
    let v = scene
        .visible(scene.bounds().unwrap())
        .find(|v| v.id.as_str() == id)
        .unwrap_or_else(|| panic!("no {id} in dark/heads.board"));
    let b = v.bounds;
    ((b.x0, b.y0, b.x1 - b.x0, b.y1 - b.y0), v.origin)
}

/// Opens `dark/heads.board` headless in `appearance` and checks the inside of the dot and
/// triangle heads is `ink`, not `absent`.
async fn heads_show(appearance: &str, ink: [u8; 3], absent: [u8; 3]) {
    let pilot = Pilot::start().await;
    let dir = tempfile::TempDir::new().unwrap();
    let mut launch = launch_with("headless", &[&fixture("dark/heads.board")]);
    launch["appearance"] = json!(appearance);
    pilot.ok("launch", launch).await;
    let tree = pilot
        .ok("tree", json!({"wait_for": {"id": ["board"]}}))
        .await;
    let path = dir.path().join(format!("heads-{appearance}.png"));
    pilot.ok("screenshot", json!({"save_to": path})).await;
    let shot = decode(&path);
    let found = leaves(&tree);

    let mut wrong = Vec::new();
    for (id, (lx, ly)) in HEADS {
        let leaf = format!("shape-{id}");
        let screen = found
            .iter()
            .find(|n| segment(n) == leaf)
            .map(|n| rect(n))
            .unwrap_or_else(|| panic!("no {leaf}: {tree}"));
        // The leaf is the arrow's world bounds through the camera, so it maps world to screen.
        let (world, start) = placed(id);
        let zoom = screen.2 / world.2;
        let to_screen = |x: f64, y: f64| {
            (
                screen.0 + (x - world.0) * zoom,
                screen.1 + (y - world.1) * zoom,
            )
        };
        let (x, y) = to_screen(start.0 + lx - PROBE / 2.0, start.1 + ly - PROBE / 2.0);
        let at = (x, y, PROBE * zoom, PROBE * zoom);
        let all = area(&shot, at);
        assert!(all > 0, "{id}: the probe is on screen: {at:?}");
        let (inked, other) = (
            near(&shot, at, ink, FLAT_TOL),
            near(&shot, at, absent, FLAT_TOL),
        );
        if inked != all || other != 0 {
            wrong.push(format!(
                "{id}: {inked} of {all} pixels ink, {other} the other ink"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{appearance}: heads are not filled with the ink {ink:02x?}: {wrong:#?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn light_fills_dot_and_triangle_heads_with_the_stored_ink() {
    heads_show("light", INK, INK_DARK).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn dark_fills_dot_and_triangle_heads_with_the_light_ink() {
    heads_show("dark", INK_DARK, INK).await;
}
