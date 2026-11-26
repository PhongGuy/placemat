//! The pilot harness shared by the scenarios: a `gpui-pilot` server in a scratch project,
//! plus helpers that read the element tree.
#![allow(dead_code)]

use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};

use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::RunningService;
use rmcp::transport::TokioChildProcess;
use rmcp::{RoleClient, ServiceExt};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::process::Command;

/// The server binary: `GPUI_PILOT_BIN`, else `gpui-pilot` on `PATH`.
pub fn server_bin() -> PathBuf {
    if let Some(bin) = std::env::var_os("GPUI_PILOT_BIN") {
        return PathBuf::from(bin);
    }
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|dir| dir.join("gpui-pilot"))
        .find(|bin| bin.is_file())
        .expect("no gpui-pilot server: install it (see gpui-pilot.toml) or set GPUI_PILOT_BIN")
}

/// The optimised app the session backend launches (gpui-pilot.toml's session build). bwrap
/// binds it at its own path, so an existing binary is canonicalised to drop any `..`.
pub fn session_app() -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    let app = target.join("timing/placemat");
    std::fs::canonicalize(&app).unwrap_or(app)
}

pub struct Pilot {
    pub client: RunningService<RoleClient, ()>,
    _root: TempDir,
    _temp: TempDir,
    /// Stand-in session bus; nothing under the pilot may connect to it.
    bus: UnixListener,
}

impl Pilot {
    /// Starts the server in a scratch project whose config launches this package's binary
    /// headless and the optimised app in a session, with a cleared environment that points
    /// the session bus and displays at canaries.
    pub async fn start() -> Pilot {
        let (root, temp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
        let config = format!(
            "[headless]\nbinary = {:?}\n\n[session]\nbinary = {:?}\n",
            env!("CARGO_BIN_EXE_placemat-pilot"),
            session_app()
        );
        std::fs::write(root.path().join("gpui-pilot.toml"), config).unwrap();
        let bus_path = temp.path().join("session_bus");
        let bus = UnixListener::bind(&bus_path).unwrap();
        bus.set_nonblocking(true).unwrap();
        let mut command = Command::new(server_bin());
        command
            .arg("mcp")
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", root.path())
            .env("TMPDIR", temp.path())
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                format!("unix:path={}", bus_path.display()),
            )
            .env("DISPLAY", ":99")
            .env("WAYLAND_DISPLAY", "gpui-pilot-canary")
            .current_dir(root.path());
        let client = ().serve(TokioChildProcess::new(command).unwrap()).await.unwrap();
        Pilot {
            client,
            _root: root,
            _temp: temp,
            bus,
        }
    }

    /// Calls `tool`, requires success, and returns the whole result.
    pub async fn call(&self, tool: &str, args: Value) -> CallToolResult {
        let Value::Object(args) = args else {
            panic!("arguments must be an object")
        };
        let result = self
            .client
            .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(args))
            .await
            .unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert_ne!(
            result.is_error,
            Some(true),
            "{tool} failed: {:?} {:?}",
            result.structured_content,
            result.content
        );
        result
    }

    pub async fn ok(&self, tool: &str, args: Value) -> Value {
        self.call(tool, args)
            .await
            .structured_content
            .expect("structured content")
    }

    pub fn assert_bus_untouched(&self) {
        match self.bus.accept() {
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            other => panic!("something connected to the session bus: {other:?}"),
        }
    }
}

/// A launch in `mode` at 1280 x 800, scale 1, without a build.
pub fn launch_args(mode: &str) -> Value {
    json!({ "mode": mode, "build": false, "width": 1280, "height": 800, "scale": 1.0 })
}

/// A board fixture from placemat-core, as an absolute path.
pub fn fixture(rel: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/placemat-core/tests/fixtures/boards")
        .join(rel);
    std::fs::canonicalize(&path).unwrap_or(path)
}

/// `launch_args(mode)` with `paths` passed to the app.
pub fn launch_with(mode: &str, paths: &[&Path]) -> Value {
    let mut args = launch_args(mode);
    args["args"] = json!(
        paths
            .iter()
            .map(|p| p.to_str().unwrap())
            .collect::<Vec<_>>()
    );
    args
}

/// The tree node for element `id`, if any: a session node carries it as `id`, a headless
/// node as the last segment of its `path`.
pub fn find_node<'a>(tree: &'a Value, id: &str) -> Option<&'a Value> {
    tree["nodes"]
        .as_array()
        .expect("tree nodes")
        .iter()
        .find(|n| {
            n["id"] == json!(id) || n["path"].as_array().and_then(|p| p.last()) == Some(&json!(id))
        })
}

pub fn node<'a>(tree: &'a Value, id: &str) -> &'a Value {
    find_node(tree, id).unwrap_or_else(|| panic!("no {id} in the tree: {tree}"))
}
