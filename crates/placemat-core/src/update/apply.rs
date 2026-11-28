//! Installing a staged update: the refusals and re-hash first, then a same-directory file
//! swap, the `.app` bundle swap, or the NSIS installer handed back to the caller. Nothing here
//! starts a process.

use std::ffi::OsString;
use std::io::Read as _;
use std::path::Path;

use sha2::{Digest as _, Sha256};

use super::bundle::{failed, install_bundle};
use super::{InstallError, InstallKind, Relaunch, Staged};

/// Installs `staged` for `kind` and says how the new version starts. Refuses `NotOwned` first,
/// then a format the kind does not accept, then a staged file that no longer hashes to
/// `staged.sha256()`. The target is unchanged and the staged file removed on every error.
pub fn install(kind: &InstallKind, staged: Staged) -> Result<Relaunch, InstallError> {
    // Refusals first, in the order a user can act on: an install the updater leaves alone,
    // then a package that does not fit it, then bytes that changed since the download.
    if let InstallKind::NotOwned(reason) = kind {
        return Err(discard(&staged, InstallError::NotOwned(*reason)));
    }
    if !kind.accepts(staged.format()) {
        return Err(discard(&staged, InstallError::WrongFormat));
    }
    if let Err(e) = rehash(&staged) {
        return Err(discard(&staged, e));
    }
    match kind {
        InstallKind::AppImage(target) => {
            replace_file(&staged, target)?;
            Ok(Relaunch::Restart(target.clone()))
        }
        InstallKind::MacApp(bundle) => {
            install_bundle(staged.path(), staged.nonce(), staged.version(), bundle)?;
            Ok(Relaunch::Restart(bundle.clone()))
        }
        InstallKind::WindowsNsis => hand_off_installer(&staged),
        InstallKind::NotOwned(reason) => Err(InstallError::NotOwned(*reason)),
    }
}

// Removes the staged file (a link itself, never a directory) and passes `e` on.
fn discard(staged: &Staged, e: InstallError) -> InstallError {
    let _ = std::fs::remove_file(staged.path());
    e
}

// lstat first, so a link swapped in after the download is refused, never followed; then the
// size and digest the download verified, read again from the bytes about to be installed.
fn rehash(staged: &Staged) -> Result<(), InstallError> {
    let path = staged.path();
    let io = |message: String| InstallError::Io {
        path: path.to_path_buf(),
        message,
    };
    if !std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file()) {
        return Err(io("the staged update is not a regular file".into()));
    }
    let mut file = std::fs::File::open(path).map_err(|e| io(e.to_string()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(io(e.to_string())),
        };
        total += n as u64;
        if total > staged.size() {
            return Err(InstallError::HashMismatch);
        }
        hasher.update(buf.get(..n).unwrap_or_default());
    }
    if total != staged.size() || hex::encode(hasher.finalize()) != staged.sha256() {
        return Err(InstallError::HashMismatch);
    }
    Ok(())
}

/// Swaps `staged` over `target`. Only a regular file beside the target is accepted, which
/// keeps the swap a same-directory rename; the target itself is never removed on failure.
fn replace_file(staged: &Staged, target: &Path) -> Result<(), InstallError> {
    let path = staged.path();
    let beside = path.parent().is_some() && path.parent() == target.parent() && path != target;
    let regular = std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file());
    let result = if beside && regular {
        swap(path, target)
    } else {
        Err(InstallError::Io {
            path: path.to_path_buf(),
            message: "the staged update is not a file beside the install target".into(),
        })
    };
    if result.is_err() && path != target {
        let _ = std::fs::remove_file(path);
    }
    result
}

/// Gives the staged file the target's permission bits (never setuid, setgid or sticky), then
/// one atomic `rename` over it.
#[cfg(unix)]
fn swap(staged: &Path, target: &Path) -> Result<(), InstallError> {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = std::fs::metadata(target)
        .map_err(|e| failed(target, target, e))?
        .permissions()
        .mode();
    std::fs::set_permissions(staged, std::fs::Permissions::from_mode(mode & 0o777))
        .map_err(|e| failed(staged, target, e))?;
    std::fs::rename(staged, target).map_err(|e| failed(target, target, e))
}

/// Off Unix no install kind reaches a file swap; this exists only so the crate compiles there.
#[cfg(not(unix))]
fn swap(staged: &Path, target: &Path) -> Result<(), InstallError> {
    std::fs::rename(staged, target).map_err(|e| failed(target, target, e))
}

/// Windows runs only a file named like an executable, so the installer is renamed to an
/// `-setup.exe` beside where it was staged. `/P` makes it passive; the app adds `/R` only to
/// relaunch.
fn hand_off_installer(staged: &Staged) -> Result<Relaunch, InstallError> {
    let path = staged.path();
    if !std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file()) {
        // Removing a link removes only the link; a directory is never removed here.
        let _ = std::fs::remove_file(path);
        return Err(InstallError::Io {
            path: path.to_path_buf(),
            message: "the staged update is not a regular file".into(),
        });
    }
    let program = path.with_file_name(format!("placemat-update-{}-setup.exe", staged.version()));
    std::fs::rename(path, &program).map_err(|e| {
        let _ = std::fs::remove_file(path);
        failed(&program, &program, e)
    })?;
    let args = vec![OsString::from("/P")];
    Ok(Relaunch::Installer { program, args })
}
