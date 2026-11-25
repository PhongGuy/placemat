# <img src="assets/icon/placemat-32.svg" alt="" width="40" height="40" align="top"> Placemat

**A repo-native whiteboard.** Placemat is a free, open-source desktop app for hand-drawn boards, diagrams with snapping connectors, ERD tables from DBML, and markdown pages that embed boards. Everything is a plain file in your own git repository, so boards are reviewed, branched and merged like code. It is written in Rust and drawn on the GPU with GPUI: no Electron, no webview. Teams will sync through a Potluck server.

## Status

Early, not usable yet. The app opens one empty window; none of the features above exist so far. The plan lives in the [issue tracker](https://github.com/PhongGuy/placemat/issues).

## Build and run

You need Rust 1.99 or newer, a C compiler, and on Linux the system libraries listed in [CONTRIBUTING.md](CONTRIBUTING.md#requirements). From a checkout:

    make run

That builds and opens the app, the same as `cargo run -p placemat-app`. `cargo build --release` puts the `placemat` binary in `target/release`.

## Contributing

Issues and pull requests from anyone are welcome, first ones included, and I'll help get a pull request over the line. Building from source, the layout, the checks, CI, the conventions for issues and pull requests, and the gpui-pilot setup are in [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Placemat is dual-licensed under either of

- the MIT license ([LICENSE-MIT](LICENSE-MIT)), or
- the Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)),

at your option. The bundled fonts and icons keep their own licenses, in [licenses/](licenses/).
