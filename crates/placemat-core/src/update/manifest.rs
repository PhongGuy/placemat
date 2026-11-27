//! The rules applied to a manifest whose signature has already been verified.

use std::collections::BTreeMap;

use semver::Version;
use serde::Deserialize;

use super::config::url_allowed;
use super::{Available, CheckError, Format, UpdateConfig};

// Unknown fields are ignored so later manifests can add to the format.
#[derive(Deserialize)]
struct Manifest {
    version: String,
    pub_date: Option<String>,
    targets: BTreeMap<String, Entry>,
}

#[derive(Deserialize)]
struct Entry {
    url: String,
    format: String,
    size: u64,
    sha256: String,
}

/// Applies the manifest rules to verified `manifest` bytes: a stable release strictly newer
/// than `cfg.current_version()` is `Some`, with the entry for `cfg.target_id()`; an older or
/// equal one is `None`. A malformed document, a pre-release or a bad entry is
/// [`CheckError::ManifestInvalid`]; a newer release without this target is
/// [`CheckError::NoEntryForTarget`].
pub fn evaluate(manifest: &[u8], cfg: &UpdateConfig) -> Result<Option<Available>, CheckError> {
    // Messages quote echoed manifest text with `{:?}`, so no control character survives.
    let invalid = CheckError::ManifestInvalid;
    let doc: Manifest = serde_json::from_slice(manifest).map_err(|e| invalid(e.to_string()))?;
    let remote = Version::parse(&doc.version)
        .map_err(|e| invalid(format!("version {:?}: {e}", doc.version)))?;
    if !remote.pre.is_empty() {
        return Err(invalid(format!("{:?} is a pre-release", doc.version)));
    }
    let current = Version::parse(cfg.current_version())
        .map_err(|e| invalid(format!("current version {:?}: {e}", cfg.current_version())))?;
    // Build metadata carries no precedence, so a rebuild of the same release is not newer.
    if remote.cmp_precedence(&current).is_le() {
        return Ok(None);
    }

    let mut targets = doc.targets;
    let entry = targets
        .remove(cfg.target_id())
        .ok_or_else(|| CheckError::NoEntryForTarget(cfg.target_id().to_owned()))?;
    let format = match entry.format.as_str() {
        "appimage" => Format::AppImage,
        "app" => Format::App,
        "nsis" => Format::Nsis,
        other => return Err(invalid(format!("unknown format {other:?}"))),
    };
    if !url_allowed(&entry.url, cfg.allow_loopback_http()) {
        return Err(invalid(format!(
            "the download URL must be https: {:?}",
            entry.url
        )));
    }
    if !is_sha256(&entry.sha256) {
        return Err(invalid(format!(
            "sha256 {:?} is not 64 lowercase hex",
            entry.sha256
        )));
    }
    Ok(Some(Available {
        version: doc.version,
        pub_date: doc.pub_date,
        format,
        url: entry.url,
        sha256: entry.sha256,
        size: entry.size,
    }))
}

/// 64 lowercase hex characters, the only digest spelling the release writes.
fn is_sha256(s: &str) -> bool {
    let lower_hex = |c: char| c.is_ascii_digit() || ('a'..='f').contains(&c);
    s.len() == 64 && s.chars().all(lower_hex)
}
