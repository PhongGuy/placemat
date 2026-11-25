# Releasing Placemat

## Overview

A release is a `v<version>` tag. Pushing it runs `.github/workflows/release.yml`, which checks that the tag matches the workspace version, that `assets/update/keys.txt` holds a real key, and that no commit since the previous tag credits an AI agent. It then builds the five targets (Linux x86_64 and arm64, Windows x64, macOS Apple silicon and Intel), adds a copy of every package under a name without the version (such as `Placemat_aarch64.dmg`, for `/releases/latest/download/` links), writes one `SHA256SUMS` over every package, writes the update manifest `latest.json`, signs it, checks the signature against the committed keys, attests every file and drafts a GitHub release. The draft's notes come from `packaging/release-notes.md`, with the version filled in and a What's Changed section taken from GitHub's generated notes since the previous tag, grouped by label through `.github/release.yml`; `scripts/release-notes.sh v<version>` prints the same notes locally with an authenticated `gh`. A human reviews the draft and publishes it. Nothing is published automatically.

No leg of the workflow has run on real runners yet. Its artifact names, signing and runner tools come from Dbento's release and are unconfirmed until the first rehearsal below has passed.

## Before the first release

- **An app icon.** linuxdeploy, the `.dmg` and the NSIS installer all want one, and none is committed yet. Add the sources and exports under `assets/icon/`, list them as `icons` in `crates/placemat-app/Cargo.toml`'s packager section, and add the 256 px export to the `.deb` layout check in `scripts/package-linux.sh`.
- **The in-app updater.** The release already writes and signs the manifest the updater reads, and `placemat-core` already holds its feed and keys, but the app does not check for updates yet. Users of a release without it update by hand.

## Who acts

Pushing a tag, publishing the draft, uploading anything and dispatching any workflow run are the maintainer's own actions, each taken only with their explicit approval.

## Signing

The build jobs compile, which runs every dependency's build script, so they get no secret and restore no cache. They set `PLACEMAT_RELEASE_BUILD=1`, and `placemat-core`'s build script then refuses to compile while `assets/update/keys.txt` holds the placeholder or no key, so no release binary can ship trusting nothing.

Linux builds run in an Ubuntu 22.04 container, so the binary needs at most glibc 2.35 and runs on 22.04 LTS and newer.

Provenance comes from a GitHub build attestation. `checksums` adds the stable copies and `SHA256SUMS`, and on a run that drafts it writes `update/latest.json` with `scripts/release.sh manifest`. It uploads the finished set, `dist/` and `update/`, as the `release-<version>` artifact. The `attest` job is the only job bound to the `release` environment, which accepts only `v*` tags and waits for the maintainer's approval, so a tag run pauses there, and only it holds `id-token: write`.

The `attest` job compiles nothing; it runs only the distro `minisign`, `base64` and shell builtins. It installs `minisign` from Ubuntu's signed archive at a pinned version before any step holds a secret. The signing key `CARGO_PACKAGER_SIGN_PRIVATE_KEY` and its password `CARGO_PACKAGER_SIGN_PRIVATE_KEY_PASSWORD` are set only on the signing step's environment; that step pipes both to `minisign`, so neither touches disk or a process's arguments. The next step verifies the signature with each key in `assets/update/keys.txt` and fails the release unless one verifies, then writes the signature as `update/latest.json.sig`. The job then attests every file, `SHA256SUMS`, `latest.json` and `latest.json.sig` included, with `actions/attest`. `draft` uploads the signed set, so every file in the release is attested.

The `.app.tar.gz` files are update-only: the updater downloads them, people do not. They live in `update/`, kept out of `dist/` and `SHA256SUMS`, so their digests are in the signed manifest and the attestation instead. The AppImages and the Windows installer serve both purposes and ship from `dist/`.

Anyone can check a download:

```sh
gh attestation verify <file> --repo PhongGuy/placemat
```

A published release's assets are never replaced; a bad release is fixed by a new version.

### macOS

There is no Apple Developer account and no notarization. The `.app` inside the `.dmg` is ad-hoc signed. Gatekeeper blocks the first open of an app that is not notarized; move `Placemat.app` to `/Applications`, then run `xattr -dr com.apple.quarantine /Applications/Placemat.app`.

### Windows

Windows files are not Authenticode-signed: that needs a CA-issued certificate, and a self-signed one only moves the SmartScreen warning.

### Linux

Linux has no OS signing. Verify downloads with `sha256sum -c SHA256SUMS`.

## Keys

The app trusts the public keys in `assets/update/keys.txt`, one base64 line each (the `.pub` file's content, base64-encoded onto one line), compiled into `placemat-core`. The current key ID is `EDC6EC51A51F1A1E`. A build only accepts a manifest signed by a key it was shipped with. `scripts/release.sh keys assets/update/keys.txt` runs the same check the release guard runs.

The private key lives online only as a secret of the `release` environment, and the key file itself stays offline as the backup, since the secret is its only online copy:

```sh
cargo packager signer generate --path placemat-update.key
gh secret set CARGO_PACKAGER_SIGN_PRIVATE_KEY --env release < placemat-update.key
gh secret set CARGO_PACKAGER_SIGN_PRIVATE_KEY_PASSWORD --env release
```

Never set them as repository secrets, which reach any workflow on any branch.

### Rotation and compromise

Rotation takes three releases. First, generate a new pair and add its public key to `keys.txt` beside the old one. Second, ship that release signed by the old key, so every client learns the new key from a manifest it can verify; then switch the release secrets to the new key. Third, remove the old line in a later release, once the new key has signed one.

If a private key leaks, every client that still lists it would accept a manifest an attacker signs with it. Delete the leaked secrets, then ship a release that removes its line from `keys.txt` and is signed by another key those clients already trust. Clients that trust only the leaked key cannot verify a release signed by the new key, so tell their users to reinstall from the Releases page.

## Rehearsal

Dispatch a dry run from `master`. It builds the chosen platforms and drafts nothing:

```sh
gh workflow run release.yml --ref master -f dry_run=true -f platforms=linux,windows,macos-arm64
gh run watch <id> --exit-status
```

The `release-<version>` artifact of the run holds every file plus `SHA256SUMS`. Download it with `gh run download <id> -n release-<version>` and run `sha256sum -c SHA256SUMS` there.

The platform tokens are `linux`, `windows`, `macos-arm64` and `macos-intel`. The default leaves out `macos-intel`, because every macOS minute bills 10x. A real release must build all four tokens, and the guard refuses a subset unless `dry_run` is set.

## Tagging

1. Set `[workspace.package] version` in `Cargo.toml` and commit.
2. `git tag v<version>`
3. `git push origin v<version>`

The guard fails on a tag that does not match the workspace version, before any build starts. A branch can only start a dry run.

## Draft review

1. Download the draft's files, run `sha256sum -c SHA256SUMS`, and run `gh attestation verify` on one of them.
2. Check the update manifest's signature against the committed key, and that its version and file names match the release:

   ```sh
   base64 -d assets/update/keys.txt > placemat-update.pub
   base64 -d latest.json.sig > latest.json.minisig
   minisign -V -p placemat-update.pub -m latest.json -x latest.json.minisig
   ```

3. Mount the `.dmg` and run `codesign -v` on the `.app`.
4. Install the `.deb` on stock Ubuntu 22.04 or 24.04; it installs `/usr/bin/placemat`.
5. Check that each package holds `THIRD-PARTY-NOTICES.html`, and that the release carries no loose copy.
6. Edit the notes: replace the intro placeholder with one to three sentences on this release's headline changes. Check it is gone; this must print `0`:

   ```sh
   gh release view v<version> --json body -q .body | grep -c '<!-- intro:'
   ```

7. Publish the release in the GitHub UI.

## Actions minutes

Each job is billed in whole minutes; Windows bills 2x and macOS 10x. No release run has been measured yet, so every job's `timeout-minutes` is a cap, not a measurement: `guard` 5, the builds 90, `checksums` 30, `attest` 15, `draft` 30. Release jobs restore no cache, so every build compiles from scratch.
