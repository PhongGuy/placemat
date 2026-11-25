<!-- intro: one to three sentences on this release's headline changes; replace this line before publishing -->

## What's Changed

{changes}

## Downloads

| Platform | File |
| --- | --- |
| Linux x86_64 | [placemat_{version}_x86_64.AppImage](https://github.com/PhongGuy/placemat/releases/download/v{version}/placemat_{version}_x86_64.AppImage) for most Linux systems, or [placemat_{version}_amd64.deb](https://github.com/PhongGuy/placemat/releases/download/v{version}/placemat_{version}_amd64.deb) for Ubuntu and Debian |
| Linux arm64 | [placemat_{version}_aarch64.AppImage](https://github.com/PhongGuy/placemat/releases/download/v{version}/placemat_{version}_aarch64.AppImage) for most Linux systems, or [placemat_{version}_arm64.deb](https://github.com/PhongGuy/placemat/releases/download/v{version}/placemat_{version}_arm64.deb) for Ubuntu and Debian |
| macOS Apple silicon | [Placemat_{version}_aarch64.dmg](https://github.com/PhongGuy/placemat/releases/download/v{version}/Placemat_{version}_aarch64.dmg) |
| macOS Intel | [Placemat_{version}_x86_64.dmg](https://github.com/PhongGuy/placemat/releases/download/v{version}/Placemat_{version}_x86_64.dmg) |
| Windows x64 | [placemat_{version}_x64-setup.exe](https://github.com/PhongGuy/placemat/releases/download/v{version}/placemat_{version}_x64-setup.exe) |

Each file also has a copy with no version in its name, for links that always point at the latest release. Check a download with `sha256sum -c SHA256SUMS`. To check that a file was built by GitHub from this project's own code, use `gh attestation verify <file> --repo PhongGuy/placemat`. Third-party licences are in `THIRD-PARTY-NOTICES.html` inside every package.

## Before you install

- **macOS:** Placemat is free and not notarized by Apple, so macOS blocks its first launch. Drag Placemat to Applications, then run `xattr -dr com.apple.quarantine /Applications/Placemat.app` once before opening it.
- **Windows:** Windows may show a blue box saying "Windows protected your PC". Placemat is free and not signed with a paid certificate, so Windows does not recognise it yet. Click "More info", then "Run anyway". Only do this for the file from this page.
- **Linux AppImage:** after downloading, right-click the file, open Properties, allow it to run as a program, then double-click it.
- **Linux .deb:** open it with your software installer and click Install.
