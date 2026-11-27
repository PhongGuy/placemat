//! Shared helpers for the updater tests. Nothing here reaches the network, the real config or
//! the environment: keys are fixtures or generated per test, and feeds are loopback servers.

/// A well-formed minisign public key line (base64 of the key box) that signs no release.
pub const KEY_LINE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDQwNzg2RTk3RUEyNjRCQzUKUldURlN5YnFsMjU0UUNFdy9pcDlBaWEyaGQ5WFd2L0tsckE2KzhQd0RFa0ZVMTJaVTllbDhMVngK";

/// The manifest target every test config uses, whatever the host.
pub const TARGET: &str = "app-linux-x86_64";

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

/// sha256("abc"), a well-formed digest for manifests whose artifact is never fetched.
pub const SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

/// A throwaway minisign keypair, generated per test.
pub struct TestKey(minisign::KeyPair);

impl TestKey {
    pub fn generate() -> TestKey {
        TestKey(minisign::KeyPair::generate_unencrypted_keypair().unwrap())
    }

    /// The public key as a `keys.txt` line: base64 of the minisign key box.
    pub fn key_line(&self) -> String {
        STANDARD.encode(self.0.pk.to_box().unwrap().to_string())
    }

    /// A `latest.json.sig` body as the release writes it: base64 of a prehashed signature box.
    pub fn sign(&self, data: &[u8]) -> String {
        let trusted = Some("timestamp:0\tfile:latest.json");
        let sig = minisign::sign(
            Some(&self.0.pk),
            &self.0.sk,
            std::io::Cursor::new(data),
            trusted,
            None,
        );
        STANDARD.encode(sig.unwrap().to_string())
    }
}

/// `sig_file` with its algorithm re-tagged from prehashed `ED` to legacy `Ed`, every other
/// byte kept, so only the algorithm check can refuse it.
pub fn retag_legacy(sig_file: &str) -> String {
    let text = String::from_utf8(STANDARD.decode(sig_file.trim()).unwrap()).unwrap();
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let mut bin = STANDARD.decode(&lines[1]).unwrap();
    assert_eq!(&bin[..2], b"ED", "minisign 0.10 signs prehashed");
    bin[1] = b'd';
    lines[1] = STANDARD.encode(&bin);
    STANDARD.encode(format!("{}\n", lines.join("\n")))
}

/// A manifest offering `version` for TARGET, with unknown fields the rules must ignore.
pub fn manifest(version: &str, format: &str, url: &str, sha: &str) -> String {
    format!(
        r#"{{"version":"{version}","pub_date":"2026-11-01T12:00:00Z","extra":1,
          "targets":{{"{TARGET}":{{"url":"{url}","format":"{format}","size":3,"sha256":"{sha}","more":true}}}}}}"#
    )
}
