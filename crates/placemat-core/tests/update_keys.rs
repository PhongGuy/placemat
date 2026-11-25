//! The release trust anchor: which update keys a build trusts, and the placeholder refusal.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use placemat_core::update::{KeysError, MANIFEST_URL, PLACEHOLDER, trusted_keys};

// A real minisign public key line (base64 of the key box), used only as well-formed input.
const KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDQwNzg2RTk3RUEyNjRCQzUKUldURlN5YnFsMjU0UUNFdy9pcDlBaWEyaGQ5WFd2L0tsckE2KzhQd0RFa0ZVMTJaVTllbDhMVngK";

#[test]
fn a_key_line_is_trusted_and_comments_and_blanks_are_skipped() {
    let text = format!("# the release key\n\n  {KEY}  \n");
    assert_eq!(trusted_keys(&text), Ok(vec![KEY.to_owned()]));
}

#[test]
fn the_placeholder_is_refused_even_beside_a_real_key() {
    assert_eq!(trusted_keys(PLACEHOLDER), Err(KeysError::Placeholder));
    let text = format!("{KEY}\n{PLACEHOLDER}\n");
    assert_eq!(trusted_keys(&text), Err(KeysError::Placeholder));
}

#[test]
fn a_file_with_no_key_is_refused() {
    assert_eq!(trusted_keys("# nothing yet\n\n"), Err(KeysError::Empty));
}

#[test]
fn a_line_that_is_not_a_minisign_key_is_refused_by_number() {
    let text = format!("{KEY}\nnot-a-key\n");
    assert_eq!(trusted_keys(&text), Err(KeysError::Malformed { line: 2 }));
}

#[test]
fn the_committed_file_holds_the_release_key() {
    let text = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/update/keys.txt"
    ));
    let keys = trusted_keys(text).unwrap();
    assert_eq!(keys.len(), 1, "{text}");
    let decoded = String::from_utf8(STANDARD.decode(&keys[0]).unwrap()).unwrap();
    assert!(
        decoded.starts_with("untrusted comment: minisign public key: EDC6EC51A51F1A1E\n"),
        "{decoded}"
    );
}

#[test]
fn the_feed_is_the_latest_release_manifest_on_github() {
    assert_eq!(
        MANIFEST_URL,
        "https://github.com/PhongGuy/placemat/releases/latest/download/latest.json"
    );
}
