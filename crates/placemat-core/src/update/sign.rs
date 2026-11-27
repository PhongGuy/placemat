//! The manifest signature check, in the format the release writes: each signature and key is
//! a minisign box, base64-encoded onto one line.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use minisign_verify::{Error, PublicKey, Signature};

use super::CheckError;

/// Manifest bytes whose signature [`verify_manifest`] accepted, borrowed from the buffer it
/// checked. Only `verify_manifest` builds one, so [`evaluate`](super::evaluate) never parses
/// unverified bytes.
///
/// ```
/// use placemat_core::update::Verified;
/// fn len(v: Verified<'_>) -> usize { v.as_bytes().len() }
/// ```
///
/// ```compile_fail
/// let forged = placemat_core::update::Verified { bytes: b"{}" };
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Verified<'a> {
    bytes: &'a [u8],
}

impl Verified<'_> {
    /// The verified manifest bytes, exactly as they were checked.
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes
    }
}

/// Verifies `manifest` against the `latest.json.sig` body (base64 of a minisign signature box,
/// surrounding whitespace ignored) with each key line (base64 of a minisign public key box).
/// A malformed, legacy or failing signature is [`CheckError::ManifestSignature`]; one whose
/// key id matches no decodable key is [`CheckError::UnknownKey`]. On success the same bytes
/// come back as [`Verified`].
pub fn verify_manifest<'a>(
    manifest: &'a [u8],
    sig_file: &[u8],
    keys: &[String],
) -> Result<Verified<'a>, CheckError> {
    let signature = unbox(sig_file)
        .and_then(|text| Signature::decode(&text).ok())
        .ok_or_else(|| {
            CheckError::ManifestSignature("the signature file is not a minisign signature".into())
        })?;
    // Only the key whose id the signature names can decide; an undecodable key line is skipped
    // so one bad entry cannot lock out the rest. `allow_legacy` false refuses non-prehashed
    // signatures before any Ed25519 work.
    for key in keys {
        let Some(key) = unbox(key.as_bytes()).and_then(|text| PublicKey::decode(&text).ok()) else {
            continue;
        };
        match key.verify(manifest, &signature, false) {
            Ok(()) => return Ok(Verified { bytes: manifest }),
            Err(Error::UnexpectedKeyId) => continue,
            Err(e) => return Err(CheckError::ManifestSignature(mid_sentence(&e.to_string()))),
        }
    }
    Err(CheckError::UnknownKey)
}

/// minisign's message, which opens with a capital, made to follow our own text after a colon;
/// an opening acronym such as "I/O" keeps its case.
fn mid_sentence(message: &str) -> String {
    let mut chars = message.chars();
    match (chars.next(), chars.next()) {
        (Some(first), Some(second)) if second.is_lowercase() => first
            .to_lowercase()
            .chain(message.chars().skip(1))
            .collect(),
        _ => message.to_owned(),
    }
}

/// The minisign box inside one base64 line, or `None` when it is not base64 of UTF-8 text.
fn unbox(line: &[u8]) -> Option<String> {
    let decoded = STANDARD.decode(line.trim_ascii()).ok()?;
    String::from_utf8(decoded).ok()
}
