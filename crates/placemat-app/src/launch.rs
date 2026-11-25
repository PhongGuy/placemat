//! The app's command line, read before GPUI starts so it works without a display.

use std::ffi::OsString;

/// What the command line asks the app to do.
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Launch {
    /// Print the usage and exit.
    Help,
    /// Print the version and exit.
    Version,
    /// Open the window.
    Open,
}

/// A command line the app refuses.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LaunchError {
    /// A flag the app does not know.
    #[error("unknown option {0}")]
    UnknownFlag(String),
    /// An argument that is not a flag; the app takes none yet.
    #[error("unexpected argument {0}")]
    UnexpectedArgument(String),
}

/// The text `--help` prints, and an unusable command line repeats.
pub const USAGE: &str = "\
Usage: placemat

Opens Placemat, a repo-native whiteboard.

Options:
  -h, --help     Print this help
  -V, --version  Print the version";

/// Reads the arguments after the program name. Help and version win over everything else;
/// any other argument is refused.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Launch, LaunchError> {
    let mut refused = None;
    for arg in args {
        match arg.to_str() {
            Some("--help" | "-h") => return Ok(Launch::Help),
            Some("--version" | "-V") => return Ok(Launch::Version),
            _ if refused.is_some() => {}
            _ if arg.as_encoded_bytes().starts_with(b"-") => {
                refused = Some(LaunchError::UnknownFlag(arg.to_string_lossy().into_owned()));
            }
            _ => {
                refused = Some(LaunchError::UnexpectedArgument(
                    arg.to_string_lossy().into_owned(),
                ));
            }
        }
    }
    refused.map_or(Ok(Launch::Open), Err)
}

/// Whether a Wayland or X11 display is named, given an environment lookup.
pub fn has_display(var: impl Fn(&str) -> Option<OsString>) -> bool {
    ["WAYLAND_DISPLAY", "DISPLAY"]
        .iter()
        .any(|v| var(v).is_some_and(|s| !s.is_empty()))
}
