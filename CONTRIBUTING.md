# Contributing to Placemat

This is the developer guide; the page for people who use Placemat is [README.md](README.md).

## Sending a change

Placemat is open source under MIT or Apache-2.0, and issues and pull requests from anyone are welcome. A first pull request is fine too, and so is a small one: a typo, a clearer error message, a missing test.

Before I merge, I check that:

- `make check` passes: the advisory scan, formatting, clippy with warnings as errors, and the tests (see [Checks and tests](#checks-and-tests)). CI runs it on every pull request, so you can lean on CI if you can't run it all locally.
- the pull request makes one change, ideally for one issue.

If something is missing, I'll say what and help get the pull request over the line. If you're not sure an idea fits, open an issue first and I'll answer there. The conventions are in [Sending a pull request](#sending-a-pull-request).

## Requirements

- Rust 1.99 or newer (the workspace's `rust-version` in `Cargo.toml`), from [rustup](https://rustup.rs).
- A C compiler.
- On Linux, the system libraries GPUI draws with. On Debian or Ubuntu, the same list CI installs:

  ```sh
  sudo apt-get install pkg-config libfontconfig-dev libfreetype-dev libwayland-dev \
    libx11-xcb-dev libxkbcommon-x11-dev libvulkan1 libegl1
  ```

Some checks need extra tools, named with each target under [Checks and tests](#checks-and-tests).

## Build from source

From a checkout, `make run` builds with 4 jobs (`make JOBS=8 run` for more) and opens the app. `cargo build --release` puts the `placemat` binary in `target/release`.

## Layout

| Path | What it is |
| --- | --- |
| `crates/placemat-core` | The UI-free core. For now it holds the trust anchor for updates: the keys in `assets/update/keys.txt` and the update feed. |
| `crates/placemat-app` | The `placemat` desktop app, built on GPUI and gpui-kit. |
| `crates/xtask` | Repository automation, run as `cargo xtask`: the third-party notices and the packager config. |
| `tools/placemat-pilot` | The gpui-pilot driver for the app, a separate workspace with its own lock. |
| `scripts/` | The packaging, release and lint scripts, with their tests in `scripts/tests`. |

## Checks and tests

`make check` is the merge gate: a RustSec advisory scan with `cargo deny` (it needs the network), `cargo fmt --check`, clippy with warnings as errors, and the test suite. `make fmt` formats both workspaces. Before you open a pull request, run `make check`, and `make pilot-check` for a change to the app.

| Target | What it does |
| --- | --- |
| `make check` | The merge gate, as above. |
| `make pilot-check` | Builds and runs the gpui-pilot scenarios in `tools/placemat-pilot`, which drive the real app. |
| `make script-test` | The bash suites under `scripts/tests`. |
| `make workflows-lint` | actionlint over `.github/workflows`. |
| `make windows-check` | Clippy on `placemat-core` for `x86_64-pc-windows-gnu`, so Windows-only code is linted from Linux. Needs that rustup target and the mingw-w64 C compiler. |
| `make icons-check` | Renders the PNG, ICO and ICNS icons from the SVG sources and fails on any stale file in `assets/icon/generated`. |
| `make notices-check` | Renders the third-party notices with `cargo-about` 0.9.2 and fails on a licence `about.toml` does not accept. |
| `make package` | The Linux `.deb` and AppImage, verified, in `target/dist/linux`. Needs `cargo-packager` 0.11.8. |
| `make run` | Builds and opens the app. |

## CI

Every push to `master` and every pull request runs the gate, macos-clippy and pilot jobs of `.github/workflows/ci.yml`, and a manual run can add the windows job:

- **gate:** `make workflows-lint script-test check windows-check icons-check notices-check` with `LOCKED=--locked`, so a stale `Cargo.lock` fails.
- **macos-clippy:** clippy on macOS.
- **pilot:** `make pilot-check` against gpui-pilot on software Vulkan.
- **windows** (manual): run the workflow from the Actions tab with `windows` ticked for clippy on the whole workspace, the core updater tests and the exe's icon and version resource check on `windows-latest`.

`zizmor.yml` audits the workflows on every push to `master` and every pull request, and `attribution.yml` fails a pull request whose title, body or commits credit an AI agent. On a schedule, `advisories.yml` scans both lockfiles daily and `weekly.yml` runs clippy on beta and a check after `cargo update`.

## Filing an issue

Open an issue with one of the templates: bug report, feature request, looks wrong or feels clunky, performance, chore, or documentation. Each one sets its label. For a bug, give the version from `placemat --version`.

## Sending a pull request

- One pull request per issue, with `Closes #N` in its body.
- The pull request carries its issue's label (`feature`, `bug`, `ux`, `accessibility`, `perf`, `docs` or `chore`); the release notes group merged pull requests by label through `.github/release.yml`, and `chore`, `ci`, `dependencies` and `skip-changelog` are left out of them.
- Give it a title in plain words, not a Conventional Commits prefix.
- Write commit messages in the [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) style, such as `fix(app): ...`, with the types `feat`, `fix`, `chore`, `refactor`, `docs` and `test`.
- Do not credit an AI agent in a commit or the pull request: no AI `Co-authored-by:` line and no "Generated with" footer. You take the credit and the responsibility for what you send. The attribution check fails a pull request that does.
- Run `make check` first, and `make pilot-check` for a change to the app.

## Pilot QA

The pilot crate (`tools/placemat-pilot`) runs the app under [gpui-pilot](https://github.com/PhongGuy/gpui-pilot) v0.7.0. `make pilot-check` needs the `gpui-pilot` server on your `PATH`, or `GPUI_PILOT_BIN` naming it. Install it with:

```sh
cargo install --locked --git https://github.com/PhongGuy/gpui-pilot --tag v0.7.0 gpui-pilot
```

`gpui-pilot.toml` tells the pilot how to build and launch Placemat. Headless mode runs `tools/placemat-pilot` against a fresh data directory; session mode runs the optimised `placemat` from the `timing` profile in a private KDE KWin session. `.mcp.json` registers the server for an agent working in this repository.

## Releases

How a release is cut and checked is in [RELEASING.md](RELEASING.md).
