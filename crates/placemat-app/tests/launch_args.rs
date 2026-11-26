//! The app's command line: the flags and the display check.

use std::ffi::OsString;

use placemat_app::launch::{Launch, LaunchError, has_display, parse};

fn args(list: &[&str]) -> Vec<OsString> {
    list.iter().map(OsString::from).collect()
}

#[test]
fn no_arguments_open_the_window_with_no_board() {
    assert_eq!(parse(args(&[])), Ok(Launch::Open { path: None }));
}

#[test]
fn one_positional_argument_is_the_board_to_open() {
    assert_eq!(
        parse(args(&["boards/x.board"])),
        Ok(Launch::Open {
            path: Some("boards/x.board".into())
        })
    );
}

#[test]
fn help_flags_ask_for_usage() {
    assert_eq!(parse(args(&["--help"])), Ok(Launch::Help));
    assert_eq!(parse(args(&["-h"])), Ok(Launch::Help));
}

#[test]
fn version_flags_ask_for_the_version_and_win_over_a_bad_flag() {
    assert_eq!(parse(args(&["--version"])), Ok(Launch::Version));
    assert_eq!(parse(args(&["-V"])), Ok(Launch::Version));
    assert_eq!(parse(args(&["-V", "--bogus"])), Ok(Launch::Version));
}

#[test]
fn an_unknown_flag_is_refused() {
    assert_eq!(
        parse(args(&["--bogus"])),
        Err(LaunchError::UnknownFlag("--bogus".into()))
    );
}

#[test]
fn a_second_path_is_refused() {
    assert_eq!(
        parse(args(&["a.board", "b.board"])),
        Err(LaunchError::UnexpectedArgument("b.board".into()))
    );
}

#[test]
fn an_unknown_flag_beside_a_path_is_refused() {
    assert_eq!(
        parse(args(&["a.board", "--bogus"])),
        Err(LaunchError::UnknownFlag("--bogus".into()))
    );
}

#[test]
fn the_usage_names_the_path_argument() {
    assert!(
        placemat_app::launch::USAGE.starts_with("Usage: placemat [PATH]"),
        "{}",
        placemat_app::launch::USAGE
    );
}

#[test]
fn a_display_is_wayland_or_x11_and_never_an_empty_value() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |v: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == v)
                .map(|(_, val)| OsString::from(val))
        }
    };
    assert!(has_display(env(&[("WAYLAND_DISPLAY", "wayland-0")])));
    assert!(has_display(env(&[("DISPLAY", ":0")])));
    assert!(!has_display(env(&[("DISPLAY", "")])));
    assert!(!has_display(env(&[])));
}
