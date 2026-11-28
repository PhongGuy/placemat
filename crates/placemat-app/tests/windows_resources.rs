//! The Windows build of `placemat.exe` carries the Placemat icon and version resources.
#![cfg(windows)]
// Test helpers may panic on a malformed exe; clippy's in-tests exemption covers only #[test] fns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use object::LittleEndian as LE;
use object::pe;
use object::read::pe::PeFile64;

const ICO: &[u8] = include_bytes!("../../../assets/icon/generated/placemat.ico");

fn exe() -> Vec<u8> {
    std::fs::read(env!("CARGO_BIN_EXE_placemat")).unwrap()
}

/// Every payload of one resource type, walking type -> name -> language as the PE tree nests them.
fn payloads(exe: &[u8], kind: u16) -> Vec<Vec<u8>> {
    let file = PeFile64::parse(exe).unwrap();
    let sections = file.section_table();
    let dir = file
        .data_directories()
        .resource_directory(exe, &sections)
        .unwrap()
        .expect("placemat.exe has no resource directory");
    let mut found = Vec::new();
    for ty in dir.root().unwrap().entries {
        if ty.name_or_id().id() != Some(kind) {
            continue;
        }
        let names = ty
            .data(dir)
            .unwrap()
            .table()
            .expect("type entry is a table");
        for name in names.entries {
            let langs = name
                .data(dir)
                .unwrap()
                .table()
                .expect("name entry is a table");
            for lang in langs.entries {
                let entry = lang
                    .data(dir)
                    .unwrap()
                    .data()
                    .expect("language entry is data");
                let size = entry.size.get(LE) as usize;
                let bytes = sections
                    .pe_data_at(exe, entry.offset_to_data.get(LE))
                    .expect("resource payload lies outside every section");
                found.push(bytes[..size].to_vec());
            }
        }
    }
    found
}

/// The image blobs of an .ico file, from its ICONDIR (6 bytes) and 16-byte ICONDIRENTRYs.
fn ico_images(ico: &[u8]) -> Vec<Vec<u8>> {
    let count = usize::from(u16::from_le_bytes([ico[4], ico[5]]));
    (0..count)
        .map(|i| {
            let entry = &ico[6 + 16 * i..6 + 16 * (i + 1)];
            let len = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize;
            let offset = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as usize;
            ico[offset..offset + len].to_vec()
        })
        .collect()
}

fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn one_icon_group_lists_every_image_in_the_ico() {
    let groups = payloads(&exe(), pe::RT_GROUP_ICON);
    assert_eq!(groups.len(), 1, "expected exactly one RT_GROUP_ICON");
    // GRPICONDIR shares ICONDIR's 6-byte header, so the image counts sit at the same offset.
    assert_eq!(
        groups[0][4..6],
        ICO[4..6],
        "group image count differs from the .ico"
    );
}

#[test]
fn the_embedded_icon_images_are_the_committed_ico() {
    let mut embedded = payloads(&exe(), pe::RT_ICON);
    let mut expected = ico_images(ICO);
    assert_eq!(expected.len(), 7, "placemat.ico should hold 7 sizes");
    embedded.sort();
    expected.sort();
    assert!(
        embedded == expected,
        "RT_ICON payloads differ from placemat.ico's images"
    );
}

#[test]
fn the_version_info_names_placemat_and_the_crate_version() {
    let versions = payloads(&exe(), pe::RT_VERSION);
    assert_eq!(versions.len(), 1, "expected exactly one RT_VERSION");
    for text in [
        "Placemat",
        "placemat.exe",
        "The Placemat authors",
        env!("CARGO_PKG_VERSION"),
    ] {
        assert!(
            contains(&versions[0], &utf16le(text)),
            "VERSIONINFO lacks {text:?}"
        );
    }
}
