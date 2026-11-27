//! Installing the staged update as the app quits, then relaunching when the user asked.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use placemat_core::update::{InstallError, InstallKind, Os, Relaunch};

use super::model::UpdateModel;

/// Starts the processes that follow an install; tests pass a fake.
pub trait Launcher {
    /// Starts the app at `path` once this process has exited.
    fn restart(&self, path: &Path) -> std::io::Result<()>;
    /// Starts the installer `program` with `args` now.
    fn start_installer(&self, program: &Path, args: &[OsString]) -> std::io::Result<()>;
}

// Long enough for a normal quit; a pid that never exits (or was reused) must not keep the
// new version from starting.
const RELAUNCH_WAIT: Duration = Duration::from_secs(30);

/// The launcher that starts real processes.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemLauncher;

impl Launcher for SystemLauncher {
    // The waiting shell runs in its own process group with no stdio, so it outlives this
    // process and the terminal it may have been started from.
    fn restart(&self, path: &Path) -> std::io::Result<()> {
        let os = Os::current().ok_or_else(|| std::io::Error::other("unsupported system"))?;
        let mut command = relaunch_command(path, std::process::id(), os, RELAUNCH_WAIT)
            .ok_or_else(|| std::io::Error::other("this system relaunches through its installer"))?;
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(drop)
    }

    fn start_installer(&self, program: &Path, args: &[OsString]) -> std::io::Result<()> {
        Command::new(program).args(args).spawn().map(drop)
    }
}

/// A shell command that waits for process `pid` to exit, at most `wait`, then starts the
/// install at `path`: the AppImage itself on Linux, the bundle through `open` on macOS.
/// `None` on Windows, where the NSIS installer relaunches the app.
pub fn relaunch_command(path: &Path, pid: u32, os: Os, wait: Duration) -> Option<Command> {
    let start = match os {
        Os::Linux => "exec \"$1\"",
        Os::Macos => "exec /usr/bin/open \"$1\"",
        Os::Windows | _ => return None,
    };
    // Every value travels as an argument ($0 pid, $1 path, $2 ticks of 100 ms), so nothing
    // is ever formatted into the script text.
    let script = format!(
        "i=0; n=$2; while [ \"$i\" -lt \"$n\" ] && kill -0 \"$0\" 2>/dev/null; \
         do sleep 0.1; i=$((i+1)); done; {start}"
    );
    let ticks = wait.as_millis() / 100;
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(script)
        .arg(pid.to_string())
        .arg(path)
        .arg(ticks.to_string());
    Some(command)
}

/// What the quit-time install did.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuitInstall {
    /// No update was staged, or no updater runs.
    Nothing,
    /// The staged update was installed, and relaunched if asked.
    Installed,
    /// The install failed with this message; nothing was launched.
    InstallFailed(String),
    /// The install succeeded but starting the next process failed with this message.
    RelaunchFailed(String),
}

/// Installs the update `model` holds armed, synchronously through `install`, then starts
/// what follows through `launcher`: the restart only when the user pressed Restart now, the
/// installer always (it is the install), given `/R` only to relaunch. `None` means no
/// updater runs, so nothing happens.
pub fn install_at_quit<A, S>(
    model: Option<&mut UpdateModel<A, S>>,
    install: impl FnOnce(&InstallKind, S) -> Result<Relaunch, InstallError>,
    launcher: &dyn Launcher,
) -> QuitInstall {
    let Some(model) = model else {
        return QuitInstall::Nothing;
    };
    let Some(armed) = model.take_armed() else {
        return QuitInstall::Nothing;
    };
    let relaunch = match install(model.kind(), armed.staged) {
        Ok(relaunch) => relaunch,
        Err(e) => return QuitInstall::InstallFailed(e.to_string()),
    };
    let launched = match relaunch {
        Relaunch::Restart(path) if armed.restart => launcher.restart(&path),
        Relaunch::Installer { program, mut args } => {
            if armed.restart {
                args.push(OsString::from("/R"));
            }
            launcher.start_installer(&program, &args)
        }
        Relaunch::Restart(_) | _ => Ok(()),
    };
    match launched {
        Ok(()) => QuitInstall::Installed,
        Err(e) => QuitInstall::RelaunchFailed(e.to_string()),
    }
}
