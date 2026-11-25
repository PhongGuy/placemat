//! The release trust anchor: the update feed and the keys a build trusts to sign it. The
//! updater that reads the feed is not built yet; the release workflow already signs for it.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

/// The marker line `assets/update/keys.txt` held before a real key existed. A release build
/// refuses to compile with it (see this crate's build script), and so does [`trusted_keys`].
pub const PLACEHOLDER: &str = "PLACEMAT-UPDATE-KEY-PLACEHOLDER";

/// The update manifest of the latest published release.
pub const MANIFEST_URL: &str =
    "https://github.com/PhongGuy/placemat/releases/latest/download/latest.json";

/// The keys file compiled into this build: one base64 minisign public key per line.
pub const COMPILED_KEYS: &str = include_str!("../../../assets/update/keys.txt");

/// Why a keys file cannot be trusted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum KeysError {
    /// The file still holds [`PLACEHOLDER`].
    #[error("the update keys are still the placeholder")]
    Placeholder,
    /// The file holds no key line.
    #[error("no update key")]
    Empty,
    /// A line is not base64 of a minisign public key.
    #[error("line {line} is not a minisign public key")]
    Malformed {
        /// The 1-based line number.
        line: usize,
    },
}

/// The key lines of a keys file, each base64 of a minisign public key box, in file order.
/// Blank lines and `#` comments are skipped. The placeholder anywhere refuses the whole file,
/// so a half-finished edit can never ship, and so does any other line minisign cannot read.
pub fn trusted_keys(text: &str) -> Result<Vec<String>, KeysError> {
    let lines = text
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.trim()))
        .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'));
    let mut keys = Vec::new();
    for (number, line) in lines {
        if line == PLACEHOLDER {
            return Err(KeysError::Placeholder);
        }
        if !is_public_key(line) {
            return Err(KeysError::Malformed { line: number });
        }
        keys.push(line.to_owned());
    }
    if keys.is_empty() {
        return Err(KeysError::Empty);
    }
    Ok(keys)
}

fn is_public_key(line: &str) -> bool {
    STANDARD
        .decode(line)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .is_some_and(|text| minisign_verify::PublicKey::decode(&text).is_ok())
}
