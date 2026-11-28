//! The text the window shows for a board that failed to open, was repaired, or is read-only.

use std::path::Path;

use placemat_app::viewer::notice::{
    NOTICE_CHARS, Tickets, WARNING_LINES, clean, error_text, readonly_text, warnings_text,
};
use placemat_core::board::{ElementId, End, LoadError, OpenError, Warning};

fn id(s: &str) -> ElementId {
    ElementId::new(s).unwrap()
}

#[test]
fn a_syntax_error_names_the_file_line_and_column() {
    let e = OpenError::Load(LoadError::Syntax {
        line: 3,
        col: 7,
        msg: "unexpected end of input".into(),
    });
    let text = error_text(Path::new("/repo/boards/plan.board"), &e);
    assert!(text.starts_with("Could not open plan.board: "), "{text}");
    assert!(text.contains("line 3, column 7"), "{text}");
}

#[test]
fn an_invalid_value_names_its_line_and_column() {
    let e = OpenError::Load(LoadError::Invalid {
        id: None,
        line: 12,
        col: 5,
        msg: "expected a colour".into(),
    });
    let text = error_text(Path::new("x.board"), &e);
    assert!(text.contains("line 12, column 5"), "{text}");
}

#[test]
fn file_errors_say_what_is_wrong_with_the_path() {
    let missing = OpenError::Io(std::io::Error::from(std::io::ErrorKind::NotFound));
    assert!(error_text(Path::new("gone.board"), &missing).contains("cannot read the board"));
    assert!(error_text(Path::new("dir"), &OpenError::NotAFile).contains("not a regular file"));
    assert!(error_text(Path::new("big.board"), &OpenError::TooLarge).contains("16 MiB"));
}

#[test]
fn clean_tags_hidden_characters_and_flattens_newlines_and_tabs() {
    assert_eq!(clean("report\u{202E}txt.exe"), "report<U+202E>txt.exe");
    assert_eq!(
        clean("a\u{2066}b\u{2069}c\u{200F}d\u{061C}e\u{200E}"),
        "a<U+2066>b<U+2069>c\u{200F}d\u{061C}e\u{200E}"
    );
    assert_eq!(
        clean("one\ntwo\tthree\u{7}\u{85}"),
        "one two three<U+0007><U+0085>"
    );
}

#[test]
fn clean_caps_the_text_at_notice_chars_with_an_ellipsis() {
    let exact = "é".repeat(NOTICE_CHARS);
    assert_eq!(clean(&exact), exact);
    let cut = clean(&"é".repeat(NOTICE_CHARS + 1));
    assert_eq!(cut.chars().count(), NOTICE_CHARS);
    assert!(cut.ends_with('\u{2026}'), "{cut}");
}

#[test]
fn error_text_is_cleaned() {
    let e = OpenError::Load(LoadError::Invalid {
        id: None,
        line: 1,
        col: 1,
        msg: format!("unknown value `{}`", "\u{202E}x".repeat(500)),
    });
    let text = error_text(Path::new("x.board"), &e);
    assert!(!text.contains('\u{202E}'), "{text}");
    assert!(
        text.chars().count() <= NOTICE_CHARS,
        "{}",
        text.chars().count()
    );
}

#[test]
fn a_file_name_is_cleaned() {
    let text = error_text(Path::new("plan\u{202E}draob.exe"), &OpenError::NotAFile);
    assert!(!text.contains('\u{202E}'), "{text}");
    assert!(text.contains("plan<U+202E>draob.exe"), "{text}");
}

#[test]
fn a_banner_cut_never_splits_a_tag() {
    let tag = "<U+202E>";
    assert_eq!(
        clean(&"\u{202E}".repeat(30)),
        tag.repeat(30),
        "exactly 240: no ellipsis"
    );
    let room = (NOTICE_CHARS - 1) / tag.len();
    assert_eq!(
        clean(&"\u{202E}".repeat(31)),
        format!("{}\u{2026}", tag.repeat(room))
    );
    let shifted = clean(&format!("ab{}", "\u{202E}".repeat(100)));
    assert_eq!(shifted, format!("ab{}\u{2026}", tag.repeat(29)));
    assert!(shifted.chars().count() <= NOTICE_CHARS, "{shifted}");
}

#[test]
fn a_huge_input_is_cut_at_the_output_bound() {
    // Several MiB in one shown span and in one run of hidden code points: the bound is on
    // output, so both come back at most NOTICE_CHARS chars.
    let plain = clean(&"x".repeat(8 << 20));
    assert_eq!(plain, format!("{}\u{2026}", "x".repeat(NOTICE_CHARS - 1)));
    let hidden = clean(&"\u{202E}".repeat(2 << 20));
    assert_eq!(hidden, format!("{}\u{2026}", "<U+202E>".repeat(29)));
    let mixed = clean(&format!("{}\u{202E}", "\u{a}".repeat(4 << 20)));
    assert_eq!(mixed, format!("{}\u{2026}", " ".repeat(NOTICE_CHARS - 1)));
}

#[test]
fn each_warning_names_what_was_repaired() {
    let text = warnings_text(&[
        Warning::DuplicateId {
            original: id("a1Xq"),
            renamed: id("a1Xq-2"),
        },
        Warning::DanglingBinding {
            arrow: id("k4Ar"),
            end: End::To,
            target: id("zz9"),
        },
        Warning::DanglingBinding {
            arrow: id("m2Zz"),
            end: End::From,
            target: id("b7Kp"),
        },
        Warning::SeedClamped { id: id("s1") },
        Warning::InvalidZ { id: id("z1") },
    ])
    .unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 5, "{lines:?}");
    assert!(
        lines[0].contains("a1Xq") && lines[0].contains("a1Xq-2"),
        "{}",
        lines[0]
    );
    assert!(lines[0].contains("renamed"), "{}", lines[0]);
    assert!(
        lines[1].contains("k4Ar") && lines[1].contains("zz9"),
        "{}",
        lines[1]
    );
    assert!(lines[1].contains("its end"), "{}", lines[1]);
    assert!(
        lines[2].contains("its start") && lines[2].contains("b7Kp"),
        "{}",
        lines[2]
    );
    assert!(lines[3].contains("seed of s1"), "{}", lines[3]);
    assert!(lines[4].contains("z1"), "{}", lines[4]);
}

#[test]
fn the_team_limit_warning_gives_the_size_in_kib() {
    let text = warnings_text(&[Warning::OverTeamLimit { bytes: 3 << 20 }]).unwrap();
    assert!(
        text.contains("3072 KiB") && text.contains("1024 KiB"),
        "{text}"
    );
}

#[test]
fn a_long_warning_list_shows_the_first_lines_and_a_count() {
    let many: Vec<Warning> = (0..WARNING_LINES + 2)
        .map(|i| Warning::SeedClamped {
            id: id(&format!("e{i}")),
        })
        .collect();
    let text = warnings_text(&many).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), WARNING_LINES + 1, "{lines:?}");
    assert_eq!(lines[WARNING_LINES], "and 2 more");
    assert!(warnings_text(&[]).is_none());
}

#[test]
fn read_only_says_a_newer_placemat_made_it() {
    let text = readonly_text(7);
    assert!(text.contains("made by a newer Placemat"), "{text}");
    assert!(
        text.contains("version 7") && text.contains("read-only"),
        "{text}"
    );
}

#[test]
fn only_the_latest_ticket_is_current() {
    let mut tickets = Tickets::default();
    let first = tickets.issue();
    assert!(tickets.is_latest(first));
    let second = tickets.issue();
    assert!(!tickets.is_latest(first));
    assert!(tickets.is_latest(second));
}

#[test]
fn a_dialog_failure_supersedes_an_open_still_in_flight() {
    let mut tickets = Tickets::default();
    let earlier = tickets.issue();
    let text = tickets.dialog_failed(&"no portal");
    assert_eq!(
        text, "Could not show the file dialog: no portal",
        "banner text"
    );
    assert!(
        !tickets.is_latest(earlier),
        "a stale open's success must not clear the dialog error"
    );
}
