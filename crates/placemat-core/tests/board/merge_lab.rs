//! The merge lab: twelve two-branch edits of one board, merged by the system git.

use std::{
    fs,
    path::Path as FsPath,
    process::{Command, Output, Stdio},
};

use placemat_core::board::{
    Anchor, Arrow, ArrowEnd, Board, Boxed, Colour, Element, End, Kind, Point, Size, Text, Warning,
    load, write,
};

use crate::util::{el, id, z};

/// Runs git with nothing from the user's environment or config: a cleared env, a home and
/// global config inside the lab, fixed identities and dates, and no line-ending conversion.
fn git(lab: &FsPath, args: &[&str]) -> Output {
    let mut cmd = Command::new("git");
    cmd.env_clear()
        .current_dir(lab.join("repo"))
        .args([
            "-c",
            "core.autocrlf=false",
            "-c",
            "init.defaultBranch=master",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", lab.join("home"))
        .env("XDG_CONFIG_HOME", lab.join("home/.config"))
        .env("GIT_CONFIG_GLOBAL", lab.join("gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "Merge Lab")
        .env("GIT_AUTHOR_EMAIL", "lab@example.invalid")
        .env("GIT_COMMITTER_NAME", "Merge Lab")
        .env("GIT_COMMITTER_EMAIL", "lab@example.invalid")
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
        .stdin(Stdio::null());
    if let Some(root) = std::env::var_os("SystemRoot") {
        cmd.env("SystemRoot", root);
    }
    cmd.output()
        .unwrap_or_else(|e| panic!("the merge lab needs git on PATH and found none: {e}"))
}

fn ok(lab: &FsPath, args: &[&str]) {
    let out = git(lab, args);
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn git_version() -> String {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|e| format!("git --version failed: {e}"))
}

fn base() -> Board {
    let mut b = Board::new();
    for (i, k, x, y, w, h, s, zk) in [
        ("a1Xq", "rect", 100, 100, 200, 100, 1001, "a1"),
        ("b7Kp", "ellipse", 400, 100, 160, 100, 1002, "a2"),
        ("c3Rt", "diamond", 700, 100, 120, 120, 1003, "a3"),
        ("d9Lm", "rect", 100, 400, 200, 100, 1004, "a4"),
        ("f5Hn", "rect", 700, 400, 200, 100, 1006, "a6"),
    ] {
        let boxed = Boxed::new(Point::new(x, y), Size::new(w, h));
        let kind = match k {
            "rect" => Kind::Rect(boxed),
            "ellipse" => Kind::Ellipse(boxed),
            _ => Kind::Diamond(boxed),
        };
        b.insert(el(i, s, zk, kind)).unwrap();
    }
    b.insert(el(
        "e2Vw",
        1005,
        "a5",
        Kind::Text(Text::new(Point::new(400, 400), "note".to_owned())),
    ))
    .unwrap();
    b
}

fn edit(b: &mut Board, i: &str, f: impl FnOnce(&mut Element)) {
    let mut e = b.get(&id(i)).unwrap().clone();
    f(&mut e);
    b.insert(e).unwrap();
}

fn boxed(e: &mut Element) -> &mut Boxed {
    let (Kind::Rect(x) | Kind::Ellipse(x) | Kind::Diamond(x)) = &mut e.kind else {
        panic!("{} is not boxed", e.id)
    };
    x
}

fn set_x(b: &mut Board, i: &str, v: i32) {
    edit(b, i, |e| boxed(e).at.x = v)
}
fn set_y(b: &mut Board, i: &str, v: i32) {
    edit(b, i, |e| boxed(e).at.y = v)
}
fn move_to(b: &mut Board, i: &str, x: i32, y: i32) {
    edit(b, i, |e| boxed(e).at = Point::new(x, y))
}
fn resize(b: &mut Board, i: &str, w: i32, h: i32) {
    edit(b, i, |e| boxed(e).size = Size::new(w, h))
}
fn recolour(b: &mut Board, i: &str) {
    edit(b, i, |e| e.stroke.colour = Colour::rgb(0xe0, 0x31, 0x31))
}
fn delete(b: &mut Board, i: &str) {
    b.remove(&id(i)).unwrap();
}
fn add(b: &mut Board, i: &str, zk: &str) {
    let s = 2000 + u32::try_from(b.len()).unwrap();
    b.insert(el(
        i,
        s,
        zk,
        Kind::Rect(Boxed::new(Point::new(1000, 100), Size::new(200, 100))),
    ))
    .unwrap();
}
fn add_bound_arrow(b: &mut Board) {
    let from = ArrowEnd::Bound {
        target: id("b7Kp"),
        anchor: Anchor::new(5000, 5000),
    };
    b.insert(el(
        "k4Ar",
        2100,
        "a7",
        Kind::Arrow(Arrow::new(from, ArrowEnd::Free(Point::new(1000, 100)))),
    ))
    .unwrap();
}

struct Scenario {
    name: &'static str,
    a: fn(&mut Board),
    b: fn(&mut Board),
    conflicts: bool,
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "S1 move two non-adjacent",
            a: |b| move_to(b, "a1Xq", 120, 130),
            b: |b| set_x(b, "c3Rt", 720),
            conflicts: false,
        },
        Scenario {
            name: "S2 move two adjacent",
            a: |b| set_x(b, "a1Xq", 120),
            b: |b| set_x(b, "b7Kp", 420),
            conflicts: false,
        },
        Scenario {
            name: "S3 both add into the same gap",
            a: |b| add(b, "g1Aa", "a7"),
            b: |b| add(b, "h1Bb", "a7V"),
            conflicts: true,
        },
        Scenario {
            name: "S4 both add into different gaps",
            a: |b| add(b, "a5Zz", "a7"),
            b: |b| add(b, "e9Yy", "a7V"),
            conflicts: false,
        },
        Scenario {
            name: "S5 one element, different lines",
            a: |b| recolour(b, "b7Kp"),
            b: |b| set_x(b, "b7Kp", 420),
            conflicts: false,
        },
        Scenario {
            name: "S6 one element, same value",
            a: |b| set_x(b, "b7Kp", 410),
            b: |b| set_x(b, "b7Kp", 430),
            conflicts: true,
        },
        Scenario {
            name: "S7 delete vs edit",
            a: |b| delete(b, "b7Kp"),
            b: |b| set_x(b, "b7Kp", 420),
            conflicts: true,
        },
        Scenario {
            name: "S8 reorder vs move another",
            a: |b| edit(b, "a1Xq", |e| e.z = z("a7")),
            b: |b| set_x(b, "d9Lm", 150),
            conflicts: false,
        },
        Scenario {
            name: "S9 x vs y on one element",
            a: |b| set_x(b, "b7Kp", 420),
            b: |b| set_y(b, "b7Kp", 120),
            conflicts: true,
        },
        Scenario {
            name: "S10 move vs resize",
            a: |b| move_to(b, "b7Kp", 420, 120),
            b: |b| resize(b, "b7Kp", 180, 120),
            conflicts: false,
        },
        Scenario {
            name: "S11 move vs recolour",
            a: |b| move_to(b, "b7Kp", 420, 120),
            b: |b| recolour(b, "b7Kp"),
            conflicts: false,
        },
        Scenario {
            name: "S12 delete vs bind an arrow to it",
            a: |b| delete(b, "b7Kp"),
            b: add_bound_arrow,
            conflicts: false,
        },
    ]
}

/// Commits base, branch `a` and branch `b`, merges `b` into `a`, and returns whether git
/// left a conflict and the merged file (meaningful only when it did not).
fn merge(s: &Scenario) -> (bool, String) {
    let tmp = tempfile::tempdir().unwrap();
    let lab = tmp.path();
    fs::create_dir_all(lab.join("home/.config")).unwrap();
    fs::create_dir(lab.join("repo")).unwrap();
    fs::write(lab.join("gitconfig"), "").unwrap();
    let file = lab.join("repo/lab.board");
    let save = |b: &Board| fs::write(&file, write(b).unwrap()).unwrap();
    ok(lab, &["init", "-q"]);
    save(&base());
    ok(lab, &["add", "lab.board"]);
    ok(lab, &["commit", "-qm", "base"]);
    for (branch, f) in [("a", s.a), ("b", s.b)] {
        ok(lab, &["checkout", "-q", "-b", branch, "master"]);
        let mut board = base();
        f(&mut board);
        save(&board);
        ok(lab, &["commit", "-qam", branch]);
    }
    ok(lab, &["checkout", "-q", "a"]);
    let out = git(lab, &["merge", "-q", "--no-edit", "b"]);
    let unmerged = git(lab, &["diff", "--name-only", "--diff-filter=U"]);
    let conflicted = !String::from_utf8_lossy(&unmerged.stdout).trim().is_empty();
    assert_eq!(
        out.status.success(),
        !conflicted,
        "{}: merge failed without a conflict: {}",
        s.name,
        String::from_utf8_lossy(&out.stderr)
    );
    (conflicted, fs::read_to_string(&file).unwrap())
}

#[test]
fn twelve_scenarios_merge_as_the_lab_measured() {
    let version = git_version();
    let mut clean = 0;
    for s in scenarios() {
        let (conflicted, merged) = merge(&s);
        assert_eq!(conflicted, s.conflicts, "{} ({version})", s.name);
        if conflicted {
            continue;
        }
        clean += 1;
        let loaded = load(merged.as_bytes()).unwrap_or_else(|e| {
            panic!("{} merged to an unloadable board: {e} ({version})", s.name)
        });
        if s.name.starts_with("S12") {
            assert_eq!(
                loaded.warnings,
                [Warning::DanglingBinding {
                    arrow: id("k4Ar"),
                    end: End::From,
                    target: id("b7Kp")
                }]
            );
        } else {
            assert!(
                loaded.warnings.is_empty(),
                "{} ({version}): {:?}",
                s.name,
                loaded.warnings
            );
            assert_eq!(
                write(&loaded.board).unwrap(),
                merged,
                "{} merged clean but not canonical ({version})",
                s.name
            );
        }
    }
    assert_eq!(clean, 8, "({version})");
}
