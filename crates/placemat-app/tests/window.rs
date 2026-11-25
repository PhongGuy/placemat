//! The main window's identity: its title, its icon and the app id the desktop shell matches
//! it by.

use std::sync::Arc;

use placemat_app::app::{APP_ID, window_icon, window_options};

#[test]
fn the_window_is_titled_placemat_and_carries_the_app_id() {
    let options = window_options();
    let title = options.titlebar.and_then(|t| t.title);
    assert_eq!(title.as_deref(), Some("Placemat"));
    assert_eq!(options.app_id.as_deref(), Some(APP_ID));
    assert_eq!(APP_ID, "placemat");
}

#[test]
fn the_packaged_desktop_entry_names_the_same_window_class() {
    let entry = include_str!("../packaging/placemat.desktop");
    assert!(
        entry
            .lines()
            .any(|l| l == format!("StartupWMClass={APP_ID}")),
        "{entry}"
    );
}

#[test]
fn the_window_icon_is_the_committed_256_px_png() {
    let icon = window_icon().expect("the committed PNG decodes");
    let png = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/icon/generated/png/256x256.png"),
    )
    .unwrap();
    let expected = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(icon.dimensions(), (256, 256));
    assert!(
        icon.as_raw() == expected.as_raw(),
        "pixels match the committed PNG"
    );
}

#[test]
fn the_icon_is_decoded_once_and_set_on_the_main_window() {
    let icon = window_icon().unwrap();
    assert!(Arc::ptr_eq(&icon, &window_icon().unwrap()), "decoded once");
    let options = window_options();
    assert!(Arc::ptr_eq(options.icon.as_ref().expect("icon set"), &icon));
}
