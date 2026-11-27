//! The artifact download: streamed to a private `.part`, capped at the listed size, hashed on
//! the way, and renamed into place only once its length and digest match the offer.

use std::fs::File;
use std::io::{ErrorKind, Read as _, Write as _};
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use super::claim::PREFIX;
use super::fetch::agent;
use super::{Available, DownloadError, Format, Staged};

const CHUNK: usize = 64 * 1024;

/// Streams the artifact to `<name>.part` in `dir`, then renames it to `<name>`:
/// `placemat-update-<version>` plus `.AppImage`, `.app.tar.gz` or `.exe`. The `.part` is
/// created exclusively (mode 0600 on Unix) before any request, or the result is
/// [`DownloadError::NotWritable`]. A body longer or shorter than listed is
/// [`DownloadError::SizeMismatch`], a wrong digest [`DownloadError::HashMismatch`], a 404
/// [`DownloadError::ArtifactMissing`] and any other non-success [`DownloadError::Http`]. Every
/// failure removes the `.part`. `progress(received, size)` runs after each chunk is written.
pub fn download(
    a: &Available,
    dir: &Path,
    progress: impl FnMut(u64, u64),
) -> Result<Staged, DownloadError> {
    let name = staged_name(a);
    let path = dir.join(&name);
    let part = dir.join(format!("{name}.part"));
    // A `.part` left by an earlier crash is never resumed: its bytes were never verified.
    let _ = std::fs::remove_file(&part);
    // Created exclusively, so a link planted after the remove is refused rather than followed.
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let file = options
        .open(&part)
        .map_err(|_| DownloadError::NotWritable(dir.to_path_buf()))?;
    let renamed = stream(a, file, &part, progress)
        .and_then(|()| std::fs::rename(&part, &path).map_err(|e| io(&path, &e)));
    if let Err(e) = renamed {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    Ok(Staged {
        path,
        version: a.version.clone(),
        format: a.format,
        sha256: a.sha256.clone(),
        size: a.size,
    })
}

/// The staged file name; installers recognise an AppImage, app bundle or `.exe` by its suffix.
fn staged_name(a: &Available) -> String {
    let suffix = match a.format {
        Format::AppImage => ".AppImage",
        Format::App => ".app.tar.gz",
        Format::Nsis => ".exe",
    };
    format!("{PREFIX}{}{suffix}", a.version)
}

/// Writes the body into `file`, never past `a.size` bytes, then checks its length and digest.
fn stream(
    a: &Available,
    mut file: File,
    part: &Path,
    mut progress: impl FnMut(u64, u64),
) -> Result<(), DownloadError> {
    // Plain http follows the offer's bit, copied from the config, never the URL text.
    let https_only = !a.allow_loopback_http;
    let response = agent(&a.timeouts, a.timeouts.artifact, https_only)
        .get(&a.url)
        .call()
        .map_err(transport)?;
    let status = response.status();
    if status.as_u16() == 404 {
        return Err(DownloadError::ArtifactMissing);
    }
    if !status.is_success() {
        return Err(DownloadError::Http(status.as_u16()));
    }
    // ureq's reader has no limit of its own, so this loop is the size cap.
    let mut reader = response.into_body().into_reader();
    let mut buf = vec![0u8; CHUNK];
    let mut hasher = Sha256::new();
    let mut received = 0u64;
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            // A body read error wraps the ureq error, so a timeout stays a timeout.
            Err(e) => return Err(transport(ureq::Error::from(e))),
        };
        let got = received + n as u64;
        if got > a.size {
            return Err(DownloadError::SizeMismatch {
                expected: a.size,
                got,
            });
        }
        let chunk = buf.get(..n).unwrap_or_default();
        file.write_all(chunk).map_err(|e| io(part, &e))?;
        hasher.update(chunk);
        received = got;
        progress(received, a.size);
    }
    if received != a.size {
        return Err(DownloadError::SizeMismatch {
            expected: a.size,
            got: received,
        });
    }
    file.sync_all().map_err(|e| io(part, &e))?;
    if hex::encode(hasher.finalize()) != a.sha256 {
        return Err(DownloadError::HashMismatch);
    }
    Ok(())
}

fn io(path: &Path, e: &std::io::Error) -> DownloadError {
    DownloadError::Io {
        path: PathBuf::from(path),
        message: e.to_string(),
    }
}

/// The download's twin of the check's mapping: a time limit is `Timeout`, anything else
/// (a refused http hop included) means the server could not be reached.
fn transport(e: ureq::Error) -> DownloadError {
    if matches!(e, ureq::Error::Timeout(_)) {
        DownloadError::Timeout
    } else {
        DownloadError::Offline(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::download;
    use crate::update::{Available, DownloadError, Format, Timeouts};
    use std::net::TcpListener;
    use std::time::Duration;

    // Built by hand: no release manifest can list this URL, so this proves the agent's https
    // rule follows the offer's bit rather than the URL.
    #[test]
    fn download_without_loopback_allowance_refuses_http() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let a = Available {
            version: "99.0.0".into(),
            pub_date: None,
            format: Format::AppImage,
            url: format!("http://{}/artifact", listener.local_addr().unwrap()),
            sha256: "0".repeat(64),
            size: 1,
            // Short limits, so a regression fails fast instead of waiting out 15 minutes.
            timeouts: Timeouts {
                connect: Duration::from_secs(2),
                manifest: Duration::from_secs(2),
                artifact: Duration::from_secs(2),
            },
            allow_loopback_http: false,
        };
        let got = download(&a, dir.path(), |_, _| {});
        assert!(matches!(got, Err(DownloadError::Offline(_))), "{got:?}");
        assert!(
            matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
            "the request reached the server"
        );
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            0,
            "a file was left"
        );
    }
}
