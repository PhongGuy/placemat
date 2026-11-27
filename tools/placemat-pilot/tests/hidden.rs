//! Hidden characters on a real canvas: tags in the leaf names and a painted tag background.

mod common;

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use common::{Pilot, fixture, launch_with, leaves, rect, segment};
use serde_json::{Value, json};

/// The launch width in logical pixels (`common::launch_args`).
const WINDOW_W: f64 = 1280.0;
/// The dark theme's `muted.background`.
const MUTED: [u8; 3] = [0x14, 0x14, 0x18];
/// The light theme's `muted.background`.
const MUTED_LIGHT: [u8; 3] = [0xF7, 0xF7, 0xF9];
/// How far a band pixel may stray from `muted`. The band is a solid fill, so a tight match
/// keeps glyph and emoji antialiasing that blends through `muted` out of the count.
const BAND_TOL: u8 = 1;

/// A decoded screenshot: 8-bit samples, `samples` per pixel, rows `line` bytes apart.
struct Shot {
    width: u32,
    height: u32,
    samples: usize,
    line: usize,
    data: Vec<u8>,
}

fn decode(path: &Path) -> Shot {
    let mut decoder = png::Decoder::new(BufReader::new(File::open(path).unwrap()));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().unwrap();
    let mut data = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut data).unwrap();
    let samples = info.color_type.samples();
    assert!(samples >= 3, "a colour screenshot: {:?}", info.color_type);
    Shot {
        width: info.width,
        height: info.height,
        samples,
        line: info.line_size,
        data,
    }
}

/// Pixels inside the logical box `b` within `tol` of `want` on every channel. Counting a
/// whole box, rather than probing one point, keeps glyph ink and antialiasing out of the call.
fn near(shot: &Shot, b: (f64, f64, f64, f64), want: [u8; 3], tol: u8) -> usize {
    let s = f64::from(shot.width) / WINDOW_W;
    let x0 = (b.0 * s).max(0.0) as u32;
    let y0 = (b.1 * s).max(0.0) as u32;
    let x1 = (((b.0 + b.2) * s) as u32).min(shot.width);
    let y1 = (((b.1 + b.3) * s) as u32).min(shot.height);
    let mut n = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = y as usize * shot.line + x as usize * shot.samples;
            let px = &shot.data[i..i + 3];
            if px.iter().zip(want).all(|(&a, b)| a.abs_diff(b) <= tol) {
                n += 1;
            }
        }
    }
    n
}

/// Opens the fixture headless in `appearance`, checks the tagged leaf names, then counts
/// `muted` pixels in a screenshot: a band under the tags, none under the plain text.
async fn tags_show(appearance: &str, muted: [u8; 3]) {
    let pilot = Pilot::start().await;
    let dir = tempfile::TempDir::new().unwrap();
    let mut launch = launch_with("headless", &[&fixture("hidden/labels.board")]);
    launch["appearance"] = json!(appearance);
    pilot.ok("launch", launch).await;
    let tree = pilot
        .ok("tree", json!({"wait_for": {"id": ["board"]}}))
        .await;
    let found = leaves(&tree);
    let by = |id: &str| -> &Value {
        found
            .iter()
            .find(|n| segment(n) == id)
            .copied()
            .unwrap_or_else(|| panic!("no {id}: {tree}"))
    };
    assert_eq!(by("shape-hidRlo01")["name"], json!("report<U+202E>txt.exe"));
    assert_eq!(by("shape-hidTags1")["name"], json!("<U+202E>".repeat(8)));
    assert_eq!(
        by("shape-hidText1")["name"],
        json!("مرحبا\u{200F} \u{1F3F4}\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}"),
        "marks and a well-formed flag pass through"
    );
    let path = dir.path().join(format!("hidden-{appearance}.png"));
    pilot.ok("screenshot", json!({"save_to": path})).await;
    let shot = decode(&path);
    let tags = near(&shot, rect(by("shape-hidTags1")), muted, BAND_TOL);
    let plain = near(&shot, rect(by("shape-hidText1")), muted, BAND_TOL);
    assert!(
        tags >= 500,
        "the background pass paints the tag band: {tags} muted pixels"
    );
    assert!(
        plain < 20,
        "no band where there is no tag: {plain} muted pixels"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn hidden_characters_show_as_tags_in_the_names_and_on_the_canvas() {
    tags_show("dark", MUTED).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn hidden_characters_show_as_tags_in_the_light_appearance() {
    tags_show("light", MUTED_LIGHT).await;
}
