//! The app's command line, read before GPUI starts so it works without a display.

use std::ffi::OsString;
use std::path::PathBuf;

/// What the command line asks the app to do.
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Launch {
    /// Print the usage and exit.
    Help,
    /// Print the version and exit.
    Version,
    /// Open the window, with the board at `path` when one is named.
    Open {
        /// The board file to open.
        path: Option<PathBuf>,
    },
}

/// A command line the app refuses.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LaunchError {
    /// A flag the app does not know.
    #[error(
        "unknown option '{0}' (to open a file whose name starts with '-', use: placemat -- {0} or ./{0})"
    )]
    UnknownFlag(String),
    /// A positional argument after the board path; the app opens one board.
    #[error("unexpected argument {0}")]
    UnexpectedArgument(String),
}

/// The text `--help` prints, and an unusable command line repeats.
pub const USAGE: &str = "\
Usage: placemat [PATH]

Opens Placemat, a repo-native whiteboard.

Arguments:
  [PATH]  A .board file to open
  --      End options, so PATH may start with '-'

Options:
  -h, --help     Print this help
  -V, --version  Print the version";

/// Reads the arguments after the program name. Help and version before `--` win over everything
/// else; the first `--` ends options, so every later argument is a path. The first path is the
/// board to open, and an unknown flag or a second path is refused (the first refusal wins).
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Launch, LaunchError> {
    let mut refused = None;
    let mut path = None;
    let mut operands_only = false;
    for arg in args {
        // After the first `--` nothing is an option, so a path named `--help` or `-V` opens
        // instead of asking for help or the version.
        let is_option = !operands_only && arg.as_encoded_bytes().starts_with(b"-");
        match arg.to_str() {
            _ if is_option && arg.as_encoded_bytes() == b"--" => operands_only = true,
            Some("--help" | "-h") if is_option => return Ok(Launch::Help),
            Some("--version" | "-V") if is_option => return Ok(Launch::Version),
            _ if refused.is_some() => {}
            _ if is_option => {
                refused = Some(LaunchError::UnknownFlag(arg.to_string_lossy().into_owned()));
            }
            // Taken as the OS gave it, so a non-UTF-8 path still opens.
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            _ => {
                refused = Some(LaunchError::UnexpectedArgument(
                    arg.to_string_lossy().into_owned(),
                ));
            }
        }
    }
    refused.map_or(Ok(Launch::Open { path }), Err)
}

/// Whether a Wayland or X11 display is named, given an environment lookup.
pub fn has_display(var: impl Fn(&str) -> Option<OsString>) -> bool {
    ["WAYLAND_DISPLAY", "DISPLAY"]
        .iter()
        .any(|v| var(v).is_some_and(|s| !s.is_empty()))
}
