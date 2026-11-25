//! The committed packager metadata of placemat-app, read through the real `cargo metadata`.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

/// The raw `[package.metadata.packager]` section and placemat-app's manifest directory.
fn section() -> (Value, PathBuf) {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let out = Command::new(cargo)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(xtask::workspace_root())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let metadata: Value = serde_json::from_slice(&out.stdout).unwrap();
    let package = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "placemat-app")
        .expect("placemat-app is a workspace member")
        .clone();
    let dir = Path::new(package["manifest_path"].as_str().unwrap())
        .parent()
        .unwrap()
        .to_path_buf();
    (package["metadata"]["packager"].clone(), dir)
}

#[test]
fn identity_and_binaries() {
    let (c, _) = section();
    assert_eq!(c["productName"], "Placemat");
    assert_eq!(c["identifier"], "dev.placemat.app");
    assert_eq!(c["category"], "Productivity");
    assert_eq!(c["binaries"], json!([{ "path": "placemat", "main": true }]));
    assert!(
        c.get("formats").is_none(),
        "formats are chosen per OS on the command line"
    );
}

#[test]
fn nothing_secret_or_derived_is_committed() {
    let (c, _) = section();
    for key in ["name", "version", "binariesDir", "outDir"] {
        assert!(
            c.get(key).is_none(),
            "{key} is filled by cargo xtask packager-config"
        );
    }
    assert!(
        c.get("macos").is_none() && c.get("windows").is_none(),
        "signing comes from the environment"
    );
}

#[test]
fn deb_declares_the_gpui_libraries_and_the_cpp_runtime() {
    let (c, _) = section();
    assert_eq!(
        c["deb"]["depends"],
        json!([
            "libxcb1",
            "libxkbcommon0",
            "libxkbcommon-x11-0",
            "libvulkan1",
            "libwayland-client0",
            "libwayland-egl1",
            "libegl1",
            "libstdc++6"
        ])
    );
    assert_eq!(c["deb"]["section"], "graphics");
}

#[test]
fn the_desktop_template_names_the_window_class() {
    let (c, dir) = section();
    assert_eq!(c["deb"]["desktopTemplate"], "packaging/placemat.desktop");
    let path = dir.join("packaging/placemat.desktop");
    let text = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    for want in [
        "[Desktop Entry]",
        "Exec={{exec}} {{exec_arg}}",
        "Icon={{icon}}",
        "Name={{name}}",
        "StartupWMClass=placemat",
        "Type=Application",
    ] {
        assert!(
            lines.contains(&want),
            "{want} missing from {}",
            path.display()
        );
    }
}

#[test]
fn resources_ship_every_licence_text() {
    let (c, dir) = section();
    assert_eq!(c["resources"], json!(["../../licenses/*.txt"]));
    let mut names: Vec<String> = std::fs::read_dir(dir.join("../../licenses"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".txt"))
        .collect();
    names.sort();
    assert_eq!(names, ["Inter-OFL.txt", "Lucide-ISC.txt"]);
}
