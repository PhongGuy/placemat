//! The words the user sees about updates: the notice per state and reason, and the README's
//! account of what the check sends.

use placemat_app::updates::{Notice, notice_text};
use placemat_core::update::Reason;

#[test]
fn notice_text_per_state_and_reason() {
    assert_eq!(
        notice_text(&Notice::Offered("1.2.0")),
        "Placemat 1.2.0 is available."
    );
    assert_eq!(
        notice_text(&Notice::Downloading("1.2.0")),
        "Downloading Placemat 1.2.0\u{2026}"
    );
    assert_eq!(
        notice_text(&Notice::Ready("1.2.0")),
        "Placemat 1.2.0 installs when you quit."
    );
    assert_eq!(
        notice_text(&Notice::DownloadFailed("1.2.0")),
        "Could not download Placemat 1.2.0."
    );
    let howto = |reason| {
        notice_text(&Notice::HowTo {
            version: "1.2.0",
            reason,
        })
    };
    let but = "Placemat 1.2.0 is available, but";
    let cases = [
        (
            Reason::SourceBuild,
            "it was built from source, so download releases by hand.",
        ),
        (
            Reason::SystemPackage,
            "it was installed by your package manager; download the new .deb.",
        ),
        (
            Reason::ReadOnlyVolume,
            "it runs from the disk image; move Placemat to Applications first.",
        ),
        (
            Reason::Translocated,
            "macOS runs it from a temporary copy; move Placemat to Applications first.",
        ),
        (
            Reason::Unrecognised,
            "the updater does not manage this install layout.",
        ),
    ];
    for (reason, rest) in cases {
        assert_eq!(howto(Some(reason)), format!("{but} {rest}"), "{reason:?}");
    }
    assert_eq!(
        howto(None),
        "Placemat 1.2.0 is available; download it from the release page."
    );
}

#[test]
fn notice_text_cleans_the_version() {
    let text = notice_text(&Notice::Offered("1.2.0\n\u{202E}x"));
    assert!(!text.contains('\n'), "{text}");
    assert!(text.contains("<U+202E>"), "{text}");
}

#[test]
fn the_readme_says_what_the_update_check_sends_and_how_to_stop_it() {
    let readme = include_str!("../../../README.md");
    for needle in [
        "every 24 hours",
        "IP address",
        "placemat/<version>",
        "PLACEMAT_NO_UPDATE_CHECK=1",
    ] {
        assert!(readme.contains(needle), "README.md lacks {needle:?}");
    }
}
