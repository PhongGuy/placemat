//! The in-app updater: where a build looks for releases, the keys it trusts to sign them,
//! and the check, download and install of a newer release. Every call blocks; the app runs
//! them off the UI thread.

use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the bundle has no caller until install exists")
)]
mod bundle;
mod config;
mod download;
mod error;
mod fetch;
mod install;
mod manifest;
mod sign;

pub use config::{RELEASE_BUILD, build_config, build_config_with, target_id};
pub use download::{download, sweep_staged};
pub use error::{CheckError, DownloadError, InstallError};
pub use fetch::check;
pub use install::{InstallKind, Reason, install_kind, staging_dir};
pub use manifest::evaluate;
pub use sign::verify_manifest;

/// The marker line `assets/update/keys.txt` held before a real key existed. A release build
/// refuses to compile with it (see this crate's build script), and so does [`trusted_keys`].
pub const PLACEHOLDER: &str = "PLACEMAT-UPDATE-KEY-PLACEHOLDER";

/// The update manifest of the latest published release.
pub const MANIFEST_URL: &str =
    "https://github.com/PhongGuy/placemat/releases/latest/download/latest.json";

/// The page a user downloads releases from by hand.
pub const DOWNLOAD_PAGE: &str = "https://github.com/PhongGuy/placemat/releases/latest";

/// The keys file compiled into this build: one base64 minisign public key per line.
pub const COMPILED_KEYS: &str = include_str!("../../../../assets/update/keys.txt");

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

/// An operating system a release can target.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    /// Linux.
    Linux,
    /// macOS.
    Macos,
    /// Windows.
    Windows,
}

impl Os {
    /// The system this build runs on, or `None` for one no release targets.
    pub fn current() -> Option<Os> {
        match std::env::consts::OS {
            "linux" => Some(Os::Linux),
            "macos" => Some(Os::Macos),
            "windows" => Some(Os::Windows),
            _ => None,
        }
    }
}

/// How a release artifact is packaged.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// A Linux AppImage (`appimage`).
    AppImage,
    /// A macOS app bundle, shipped as `.app.tar.gz` (`app`).
    App,
    /// A Windows NSIS installer (`nsis`).
    Nsis,
}

/// Network time limits for the update check and the artifact download.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    /// Establishing a connection.
    pub connect: Duration,
    /// Fetching the manifest and its signature, each.
    pub manifest: Duration,
    /// The whole artifact download.
    pub artifact: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Timeouts {
            connect: Duration::from_secs(10),
            manifest: Duration::from_secs(30),
            artifact: Duration::from_secs(15 * 60),
        }
    }
}

/// Everything a check needs: where the manifest lives, which keys may sign it, and what this
/// build is. Only [`build_config_with`] makes one, so the loopback allowance cannot be forged.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct UpdateConfig {
    manifest_url: String,
    download_page: String,
    keys: Vec<String>,
    current_version: String,
    target_id: String,
    timeouts: Timeouts,
    allow_loopback_http: bool,
}

impl UpdateConfig {
    /// The URL of `latest.json`.
    pub fn manifest_url(&self) -> &str {
        &self.manifest_url
    }

    /// The page a user downloads releases from by hand.
    pub fn download_page(&self) -> &str {
        &self.download_page
    }

    /// Trusted minisign public keys, one base64 `.pub` line each.
    pub fn keys(&self) -> &[String] {
        &self.keys
    }

    /// This build's version.
    pub fn current_version(&self) -> &str {
        &self.current_version
    }

    /// The manifest target this build installs from, as [`target_id`] gives it.
    pub fn target_id(&self) -> &str {
        &self.target_id
    }

    /// The network time limits.
    pub fn timeouts(&self) -> Timeouts {
        self.timeouts
    }

    /// Whether plain http to a loopback host is allowed: only for a non-release build pointed
    /// at a local feed.
    pub fn allow_loopback_http(&self) -> bool {
        self.allow_loopback_http
    }

    /// This config with other network time limits.
    #[must_use]
    pub fn with_timeouts(mut self, timeouts: Timeouts) -> Self {
        self.timeouts = timeouts;
        self
    }
}

/// A newer stable release with an artifact for this build's target.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Available {
    version: String,
    pub_date: Option<String>,
    format: Format,
    url: String,
    sha256: String,
    size: u64,
    // Copied from the config by `evaluate`, so the download keeps the check's rules.
    pub(crate) timeouts: Timeouts,
    pub(crate) allow_loopback_http: bool,
}

impl Available {
    /// The release version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The publication time, as the manifest states it.
    pub fn pub_date(&self) -> Option<&str> {
        self.pub_date.as_deref()
    }

    /// How the artifact is packaged.
    pub fn format(&self) -> Format {
        self.format
    }

    /// Where the artifact downloads from.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The artifact's SHA-256, 64 lowercase hex characters.
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// The artifact's size in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }
}

/// A downloaded artifact whose size and SHA-256 matched the offer, waiting to be installed.
#[non_exhaustive]
#[derive(Debug)]
pub struct Staged {
    path: PathBuf,
    version: String,
    format: Format,
    sha256: String,
    size: u64,
}

impl Staged {
    /// The staged file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The release version it holds.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// How it is packaged.
    pub fn format(&self) -> Format {
        self.format
    }

    /// Its SHA-256 as the offer listed it, 64 lowercase hex characters.
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// Its size in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }
}
