//! The update notice in the running app, against a loopback feed signed with a throwaway key.
//! A source build under the test feed is not an install the updater owns, so the notice offers
//! the download page, never Install. Never clicks the download page, which opens a browser.

mod common;

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use common::{Pilot, find_node, launch_args, node};
use placemat_core::update::target_id;
use serde_json::json;

/// Serves `files` (path, body) over plain HTTP on a loopback port for the rest of the test,
/// one request per connection; any other path is a 404.
fn serve(files: Vec<(&'static str, Vec<u8>)>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            if reader.read_line(&mut request).is_err() {
                continue;
            }
            // Read through the blank line so the client sees a whole exchange.
            loop {
                let mut header = String::new();
                if !matches!(reader.read_line(&mut header), Ok(n) if n > 0) || header == "\r\n" {
                    break;
                }
            }
            let at = request.split_whitespace().nth(1).unwrap_or("");
            let reply = match files.iter().find(|(p, _)| *p == at) {
                Some((_, body)) => {
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    [head.into_bytes(), body.clone()].concat()
                }
                None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_vec(),
            };
            let _ = stream.write_all(&reply);
        }
    });
    port
}

/// A signed 99.0.0 manifest for this host's target on loopback; returns its port and the
/// key line that verifies it.
fn feed() -> (u16, String) {
    let key = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let manifest = format!(
        r#"{{"version":"99.0.0","targets":{{"{}":{{"url":"http://127.0.0.1:9/artifact","format":"appimage","size":1,"sha256":"{}"}}}}}}"#,
        target_id().expect("a release target for this host"),
        "0".repeat(64)
    );
    let sig = minisign::sign(
        Some(&key.pk),
        &key.sk,
        std::io::Cursor::new(manifest.as_bytes()),
        Some("timestamp:0\tfile:latest.json"),
        None,
    )
    .unwrap();
    let port = serve(vec![
        ("/latest.json", manifest.into_bytes()),
        (
            "/latest.json.sig",
            STANDARD.encode(sig.to_string()).into_bytes(),
        ),
    ]);
    (port, STANDARD.encode(key.pk.to_box().unwrap().to_string()))
}

#[tokio::test(flavor = "multi_thread")]
async fn notice_only_for_unowned() {
    let (port, key_line) = feed();
    let pilot = Pilot::start().await;
    let mut args = launch_args("headless");
    args["env"] = json!({
        "PLACEMAT_UPDATE_MANIFEST_URL": format!("http://127.0.0.1:{port}/latest.json"),
        "PLACEMAT_TEST_UPDATE_KEYS": key_line,
    });
    pilot.ok("launch", args).await;
    pilot
        .ok(
            "wait",
            json!({"target": {"id": ["update-notice"]}, "timeout_ms": 20000}),
        )
        .await;
    let tree = pilot.ok("tree", json!({})).await;
    assert_eq!(
        node(&tree, "update-notice")["name"],
        json!(
            "Placemat 99.0.0 is available, but it was built from source, so download releases by hand."
        )
    );
    node(&tree, "update-download-page");
    node(&tree, "update-later");
    assert!(
        find_node(&tree, "update-install").is_none(),
        "a source build offered to install: {tree}"
    );
    pilot
        .ok("click", json!({"target": {"id": ["update-later"]}}))
        .await;
    pilot
        .ok(
            "wait",
            json!({"target": {"id": ["update-notice"]}, "gone": true}),
        )
        .await;
    let tree = pilot.ok("tree", json!({})).await;
    assert!(
        find_node(&tree, "update-notice").is_none(),
        "Later left the notice: {tree}"
    );
    pilot.assert_bus_untouched();
}

#[tokio::test(flavor = "multi_thread")]
async fn without_the_test_feed_a_source_build_shows_no_notice() {
    let pilot = Pilot::start().await;
    pilot.ok("launch", launch_args("headless")).await;
    pilot
        .ok("wait", json!({"target": {"id": ["open-board"]}}))
        .await;
    pilot.ok("wait", json!({"ms": 1000})).await;
    let tree = pilot.ok("tree", json!({})).await;
    assert!(find_node(&tree, "update-notice").is_none(), "{tree}");
}
