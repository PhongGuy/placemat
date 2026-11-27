//! Hidden characters on a real canvas: tags in the leaf names and a painted tag background.

mod common;

use common::{Pilot, decode, fixture, launch_with, leaves, near, rect, segment};
use serde_json::{Value, json};

/// The dark theme's `muted.background`.
const MUTED: [u8; 3] = [0x14, 0x14, 0x18];
/// The light theme's `muted.background`.
const MUTED_LIGHT: [u8; 3] = [0xF7, 0xF7, 0xF9];
/// How far a band pixel may stray from `muted`. The band is a solid fill, so a tight match
/// keeps glyph and emoji antialiasing that blends through `muted` out of the count.
const BAND_TOL: u8 = 1;

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
