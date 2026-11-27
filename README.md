# <img src="assets/icon/placemat-32.svg" alt="" width="40" height="40" align="top"> Placemat

**A repo-native whiteboard.** Placemat is a free, open-source desktop app for hand-drawn boards, diagrams with snapping connectors, ERD tables from DBML, and markdown pages that embed boards. Everything is a plain file in your own git repository, so boards are reviewed, branched and merged like code. It is written in Rust and drawn on the GPU with GPUI: no Electron, no webview. Teams will sync through a Potluck server.

## Status

Early, not usable yet. The app opens one empty window; none of the features above exist so far. The plan lives in the [issue tracker](https://github.com/PhongGuy/placemat/issues).

## Build and run

You need Rust 1.99 or newer, a C compiler, and on Linux the system libraries listed in [CONTRIBUTING.md](CONTRIBUTING.md#requirements). From a checkout:

    make run

That builds and opens the app, the same as `cargo run -p placemat-app`. `cargo build --release` puts the `placemat` binary in `target/release`.

`make install` puts `placemat` in `~/.cargo/bin` (or `PREFIX=<dir>` for `<dir>/bin`) and, on Linux, adds a desktop entry and icons under `~/.local/share`. `make uninstall` removes them.

## Updates

A release build checks GitHub for a newer release at launch and then every 24 hours while it runs. Each check fetches `latest.json` and its signature from the project's GitHub releases. The request sends your IP address and the User-Agent `placemat/<version>`, with only the `accept`, `accept-encoding` and `host` headers besides and no query string. Nothing else is sent. When a newer release exists, the app shows a notice; the release itself is downloaded only when you click Install. For an install the updater manages, it replaces the app when you quit; otherwise the notice links to the release page.

Set `PLACEMAT_NO_UPDATE_CHECK=1` in the environment to turn the check off. Builds from source, `make install` included, never check unless `PLACEMAT_UPDATE_MANIFEST_URL` points them at a test feed.

## Contributing

Issues and pull requests from anyone are welcome, first ones included, and I'll help get a pull request over the line. Building from source, the layout, the checks, CI, the conventions for issues and pull requests, and the gpui-pilot setup are in [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Placemat is dual-licensed under either of

- the MIT license ([LICENSE-MIT](LICENSE-MIT)), or
- the Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)),

at your option. The bundled fonts and icons keep their own licenses, in [licenses/](licenses/).
