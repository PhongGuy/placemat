//! Frame cost of the board canvas on the dev GPU. Not in CI: `heavy make pilot-perf`.

mod common;

use std::path::Path;

use common::{Pilot, fixture, launch_with, node, rect};
use rmcp::model::CallToolRequestParams;
use serde_json::{Value, json};

/// Calls `tool` and returns its structured result, or the error as text instead of panicking.
async fn try_ok(pilot: &Pilot, tool: &str, args: Value) -> Result<Value, String> {
    let Value::Object(args) = args else {
        return Err("arguments must be an object".into());
    };
    let params = CallToolRequestParams::new(tool.to_owned()).with_arguments(args);
    match pilot.client.call_tool(params).await {
        Ok(r) if r.is_error != Some(true) => r
            .structured_content
            .ok_or_else(|| format!("{tool}: no content")),
        Ok(r) => Err(format!(
            "{tool}: {:?} {:?}",
            r.structured_content, r.content
        )),
        Err(e) => Err(format!("{tool}: {e}")),
    }
}

/// A pan recorded in session mode: frames are damage stamps there, so only the p95 interval
/// means anything. The tree is read only before the motion, since accesskit lags behind a
/// moving tree; the pan itself uses point targets. Every step reports failure instead of
/// panicking.
async fn session_pan(pilot: &Pilot, board: &Path) -> Result<Value, String> {
    let mut args = launch_with("session", &[board]);
    args["replace"] = json!(true);
    try_ok(pilot, "launch", args).await?;
    let tree = try_ok(
        pilot,
        "tree",
        json!({"wait_for": {"id": ["board-canvas"]}, "timeout_ms": 30_000}),
    )
    .await?;
    let b = &tree["nodes"]
        .as_array()
        .and_then(|ns| {
            ns.iter().find(|n| {
                n["id"] == json!("board-canvas")
                    || n["path"].as_array().and_then(|p| p.last()) == Some(&json!("board-canvas"))
            })
        })
        .ok_or("no board-canvas in the session tree")?["bounds"];
    let (cx, cy) = (
        b["x"].as_f64().unwrap_or(0.0) + b["w"].as_f64().unwrap_or(0.0) / 2.0,
        b["y"].as_f64().unwrap_or(0.0) + b["h"].as_f64().unwrap_or(0.0) / 2.0,
    );
    try_ok(pilot, "frames", json!({"action": "start"})).await?;
    let (from, to) = (
        json!({"x": cx, "y": cy - 50.0}),
        json!({"x": cx, "y": cy + 50.0}),
    );
    try_ok(
        pilot,
        "drag",
        json!({"from": from, "to": to, "steps": 100, "settle_ms": 0}),
    )
    .await?;
    try_ok(pilot, "wait", json!({"ms": 300})).await?;
    try_ok(pilot, "frames", json!({"action": "stop"})).await
}

/// The gate: median frame work for a 1,000-visible pan, headless, scale 1, a11y off.
const GATE_MS: f64 = 16.7;

fn centre(b: (f64, f64, f64, f64)) -> (f64, f64) {
    (b.0 + b.2 / 2.0, b.1 + b.3 / 2.0)
}

/// Launches `board` (replacing any running app) and returns the host pid and canvas centre.
async fn open(pilot: &Pilot, mode: &str, board: &Path, scale: f64) -> (u64, (f64, f64)) {
    let mut args = launch_with(mode, &[board]);
    args["scale"] = json!(scale);
    args["replace"] = json!(true);
    args["settle_ms"] = json!(20_000);
    let launched = pilot.ok("launch", args).await;
    let tree = pilot
        .ok(
            "tree",
            json!({"wait_for": {"id": ["board"]}, "timeout_ms": 30_000}),
        )
        .await;
    (
        launched["pid"].as_u64().unwrap_or(0),
        centre(rect(node(&tree, "board"))),
    )
}

/// Records the frames of a 100-step, 100 px vertical drag at the canvas centre.
async fn pan(pilot: &Pilot, (cx, cy): (f64, f64), a11y: Option<bool>) -> Value {
    let mut start = json!({"action": "start"});
    if let Some(on) = a11y {
        start["a11y"] = json!(on);
    }
    pilot.ok("frames", start).await;
    let (from, to) = (
        json!({"x": cx, "y": cy - 50.0}),
        json!({"x": cx, "y": cy + 50.0}),
    );
    pilot
        .ok(
            "drag",
            json!({"from": from, "to": to, "steps": 100, "settle_ms": 0}),
        )
        .await;
    pilot.ok("wait", json!({"ms": 300})).await;
    pilot.ok("frames", json!({"action": "stop"})).await
}

/// Records twenty notches of zoom-out at the canvas centre.
async fn zoom_out(pilot: &Pilot, (cx, cy): (f64, f64)) -> Value {
    pilot.ok("frames", json!({"action": "start"})).await;
    let scroll =
        json!({"target": {"x": cx, "y": cy}, "notches": 20, "notch_ms": 16, "settle_ms": 0});
    pilot.ok("scroll", scroll).await;
    pilot.ok("wait", json!({"ms": 300})).await;
    pilot.ok("frames", json!({"action": "stop"})).await
}

fn median(report: &Value) -> f64 {
    report["work"]["median_ms"].as_f64().unwrap_or(f64::NAN)
}

fn rss_kib(pid: u64) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

/// Whole-GPU memory in MiB: this machine's nvidia-smi lists no graphics-only process.
fn gpu_mib() -> Option<u64> {
    nvidia_smi(&["--query-gpu=memory.used", "--format=csv,noheader,nounits"])
        .trim()
        .parse()
        .ok()
}

fn nvidia_smi(args: &[&str]) -> String {
    std::process::Command::new("nvidia-smi")
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|e| format!("nvidia-smi failed: {e}"))
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the dev GPU; run with heavy make pilot-perf"]
async fn board_render_perf() {
    let pilot = Pilot::start().await;
    let (small, large) = (
        fixture("perf/grid-1000.board"),
        fixture("perf/grid-10000.board"),
    );
    println!(
        "PERF gpu {:?}",
        nvidia_smi(&["--query-gpu=name,driver_version", "--format=csv,noheader"])
    );

    let idle = gpu_mib();
    let (pid, at) = open(&pilot, "headless", &small, 1.0).await;
    let gate = pan(&pilot, at, None).await;
    println!("PERF pan-1000 {gate}");
    let steady = gpu_mib();
    let with_leaves = pan(&pilot, at, Some(true)).await;
    println!("PERF pan-1000-a11y {with_leaves}");
    println!(
        "PERF a11y-delta-ms {}",
        median(&with_leaves) - median(&gate)
    );
    println!("PERF zoom-out-1000 {}", zoom_out(&pilot, at).await);

    let rss_before = rss_kib(pid);
    for notches in [-60, 120, -60] {
        pilot
            .ok(
                "scroll",
                json!({"target": {"x": at.0, "y": at.1}, "notches": notches}),
            )
            .await;
    }
    println!(
        "PERF rss-kib-full-zoom-range before={rss_before:?} after={:?}",
        rss_kib(pid)
    );
    pilot.ok("close", json!({})).await;
    let closed = gpu_mib();
    let delta = steady.zip(idle).map(|(s, i)| s as i64 - i as i64);
    println!("PERF gpu-mib idle={idle:?} steady={steady:?} closed={closed:?} delta={delta:?}");

    let (_, at) = open(&pilot, "headless", &small, 2.0).await;
    println!("PERF pan-1000-scale-2 {}", pan(&pilot, at, None).await);

    let (_, at) = open(&pilot, "headless", &large, 1.0).await;
    println!("PERF pan-10000 {}", pan(&pilot, at, None).await);
    println!("PERF zoom-out-10000 {}", zoom_out(&pilot, at).await);

    match session_pan(&pilot, &small).await {
        Ok(report) => println!("PERF pan-1000-session {report}"),
        Err(why) => println!("PERF pan-1000-session failed {why}"),
    }

    assert!(
        median(&gate) <= GATE_MS,
        "median work {} ms over the {GATE_MS} ms gate: {gate}",
        median(&gate)
    );
}
