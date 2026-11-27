//! Which installs the updater may replace, and where it stages their replacement.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::error::InstallError;
use super::{Format, Os};

/// What kind of install a running executable belongs to, and so what may replace it.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallKind {
    /// The app running from this AppImage file.
    AppImage(PathBuf),
    /// The app running from this `.app` bundle directory.
    MacApp(PathBuf),
    /// The app installed by the NSIS installer.
    WindowsNsis,
    /// An install the updater must leave alone.
    NotOwned(Reason),
}

impl InstallKind {
    /// Whether a release packaged as `format` can replace this install.
    pub fn accepts(&self, format: Format) -> bool {
        matches!(
            (self, format),
            (InstallKind::AppImage(_), Format::AppImage)
                | (InstallKind::MacApp(_), Format::App)
                | (InstallKind::WindowsNsis, Format::Nsis)
        )
    }
}

/// Why the updater leaves an install alone, worded as what the user does instead.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Reason {
    /// It was built from source rather than by the release workflow.
    #[error("it was built from source, so download releases by hand")]
    SourceBuild,
    /// A system package manager installed it.
    #[error("it was installed by your package manager; download the new .deb")]
    SystemPackage,
    /// It runs from a mounted disk image.
    #[error("it runs from the disk image; move Placemat to Applications first")]
    ReadOnlyVolume,
    /// macOS App Translocation runs it from a temporary copy.
    #[error("macOS runs it from a temporary copy; move Placemat to Applications first")]
    Translocated,
    /// No rule recognises its layout.
    #[error("the updater does not manage this install layout")]
    Unrecognised,
}

/// Classifies the install `exe` belongs to, applying the rules in order. Pure: the caller
/// passes [`RELEASE_BUILD`](super::RELEASE_BUILD), canonicalizes `exe` and passes `$APPIMAGE`
/// as `appimage_env`.
pub fn install_kind(
    os: Os,
    release_build: bool,
    exe: &Path,
    appimage_env: Option<&Path>,
) -> InstallKind {
    if !release_build {
        return InstallKind::NotOwned(Reason::SourceBuild);
    }
    let text = exe.to_string_lossy();
    // A path the caller failed to canonicalize could disguise a package-managed binary,
    // so it is never replaced.
    if !canonical(os, &text) {
        return InstallKind::NotOwned(Reason::Unrecognised);
    }
    // Judged on the text like `canonical`, since a Windows host reads `//usr` as a network share.
    let mut segments = text.split('/').filter(|seg| !seg.is_empty());
    if os == Os::Linux && segments.next() == Some("usr") && segments.next() != Some("local") {
        return InstallKind::NotOwned(Reason::SystemPackage);
    }
    app_kind(os, &text, exe, appimage_env)
}

// The app's layouts, checked so a mounted dmg or a translocated copy wins over its bundle.
fn app_kind(os: Os, text: &str, exe: &Path, appimage_env: Option<&Path>) -> InstallKind {
    if os == Os::Macos && text.starts_with("/Volumes/") {
        return InstallKind::NotOwned(Reason::ReadOnlyVolume);
    }
    if os == Os::Macos && text.contains("/AppTranslocation/") {
        return InstallKind::NotOwned(Reason::Translocated);
    }
    if let (Os::Linux, Some(appimage)) = (os, appimage_env) {
        return InstallKind::AppImage(appimage.to_path_buf());
    }
    if os == Os::Macos
        && let Some(bundle) = mac_bundle(exe)
    {
        return InstallKind::MacApp(bundle);
    }
    if os == Os::Windows {
        return InstallKind::WindowsNsis;
    }
    InstallKind::NotOwned(Reason::Unrecognised)
}

/// Whether `text` is absolute for `os` and free of `.` and `..` segments. Judged on the text
/// rather than the host's path rules, since `os` need not be the host.
fn canonical(os: Os, text: &str) -> bool {
    let (absolute, separators): (bool, &[char]) = match os {
        Os::Windows => {
            let drive =
                matches!(text.as_bytes(), [d, b':', b'\\' | b'/', ..] if d.is_ascii_alphabetic());
            (drive || text.starts_with(r"\\"), &['\\', '/'])
        }
        Os::Linux | Os::Macos => (text.starts_with('/'), &['/']),
    };
    absolute && !text.split(separators).any(|seg| seg == "." || seg == "..")
}

/// The `X.app` directory when `exe` sits at `X.app/Contents/MacOS/<exe>`.
fn mac_bundle(exe: &Path) -> Option<PathBuf> {
    let named = |dir: &&Path, name: &str| dir.file_name() == Some(OsStr::new(name));
    let macos = exe.parent().filter(|d| named(d, "MacOS"))?;
    let contents = macos.parent().filter(|d| named(d, "Contents"))?;
    let app = contents.parent()?;
    let is_app = app.file_name()?.to_string_lossy().ends_with(".app");
    is_app.then(|| app.to_path_buf())
}

/// The directory a download for `kind` is staged in: beside the AppImage or bundle, so the
/// final swap is a rename on one volume, or the temp dir for an NSIS installer. Refuses an
/// install the updater does not own, and a file or bundle path with no parent.
pub fn staging_dir(kind: &InstallKind) -> Result<PathBuf, InstallError> {
    match kind {
        InstallKind::AppImage(p) | InstallKind::MacApp(p) => p
            .parent()
            .filter(|d| !d.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .ok_or(InstallError::NotOwned(Reason::Unrecognised)),
        InstallKind::WindowsNsis => Ok(std::env::temp_dir()),
        InstallKind::NotOwned(r) => Err(InstallError::NotOwned(*r)),
    }
}
