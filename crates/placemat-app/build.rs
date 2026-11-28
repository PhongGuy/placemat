//! Embeds the application icon and version info into Windows builds; a no-op for every other target.

use std::{env, io, path::Path};

fn main() -> io::Result<()> {
    // An absolute path, because resource compilers resolve relative paths from their own cwd.
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").map_err(io::Error::other)?;
    let icon = Path::new(&manifest_dir).join("../../assets/icon/generated/placemat.ico");
    let icon = icon
        .to_str()
        .ok_or_else(|| io::Error::other("icon path is not UTF-8"))?;
    println!("cargo:rerun-if-changed={icon}");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");

    // The build script runs on the host, so the target comes from cargo's cfg variable.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set("ProductName", "Placemat")
            .set("FileDescription", "Placemat")
            .set("OriginalFilename", "placemat.exe")
            .set("InternalName", "placemat")
            .set("LegalCopyright", "Copyright (c) The Placemat authors")
            .set("CompanyName", "The Placemat authors")
            .set_icon(icon)
            .compile()?;
    }
    Ok(())
}
