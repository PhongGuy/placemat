//! Installing at quit: synchronous, only with a staged update, and relaunching only for
//! Restart now after a successful install.

use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use placemat_app::updates::{
    Launcher, Offer, QuitInstall, UpdateModel, install_at_quit, relaunch_command,
};
use placemat_core::update::{Format, InstallError, InstallKind, Os, Relaunch};

type Model = UpdateModel<&'static str, &'static str>;

#[derive(Debug, PartialEq, Eq)]
enum Call {
    Restart(PathBuf),
    Installer(PathBuf, Vec<OsString>),
}

#[derive(Default)]
struct FakeLauncher {
    calls: RefCell<Vec<Call>>,
    fail: bool,
}

impl Launcher for FakeLauncher {
    fn restart(&self, path: &Path) -> std::io::Result<()> {
        self.calls
            .borrow_mut()
            .push(Call::Restart(path.to_path_buf()));
        if self.fail {
            return Err(std::io::Error::other("no shell"));
        }
        Ok(())
    }

    fn start_installer(&self, program: &Path, args: &[OsString]) -> std::io::Result<()> {
        let call = Call::Installer(program.to_path_buf(), args.to_vec());
        self.calls.borrow_mut().push(call);
        Ok(())
    }
}

const TARGET: &str = "/opt/Placemat.AppImage";

fn appimage() -> InstallKind {
    InstallKind::AppImage(PathBuf::from(TARGET))
}

/// A model holding a staged 1.2.0 for `kind`, offered as `format`, with Restart now pressed
/// when `restart`.
fn staged(kind: InstallKind, format: Format, restart: bool) -> Model {
    let mut m = Model::new(kind);
    m.checked(Ok(Some(Offer::new("1.2.0", format, "artifact"))));
    assert!(
        m.start_download().is_some(),
        "the fixture's kind accepts its format"
    );
    m.downloaded(Some("staged-file"));
    if restart {
        assert!(m.restart_now(), "a staged model restarts");
    }
    m
}

fn restarts() -> Result<Relaunch, InstallError> {
    Ok(Relaunch::Restart(PathBuf::from(TARGET)))
}

#[test]
fn quit_installs_synchronously_when_staged() {
    let mut m = staged(appimage(), Format::AppImage, false);
    let ran = Cell::new(false);
    let launcher = FakeLauncher::default();
    let install = |kind: &InstallKind, s: &str| {
        assert_eq!(kind, &appimage());
        assert_eq!(s, "staged-file");
        ran.set(true);
        restarts()
    };
    let got = install_at_quit(Some(&mut m), install, &launcher);
    assert!(ran.get(), "the install ran before install_at_quit returned");
    assert_eq!(got, QuitInstall::Installed);
    assert!(m.take_armed().is_none(), "the install is consumed");
}

#[test]
fn quit_without_staged_does_nothing() {
    let mut fresh = Model::new(appimage());
    let mut offered = Model::new(appimage());
    offered.checked(Ok(Some(Offer::new("1.2.0", Format::AppImage, "artifact"))));
    let mut downloading = Model::new(appimage());
    downloading.checked(Ok(Some(Offer::new("1.2.0", Format::AppImage, "artifact"))));
    assert!(
        downloading.start_download().is_some(),
        "an offered release downloads"
    );
    let launcher = FakeLauncher::default();
    for m in [&mut fresh, &mut offered, &mut downloading] {
        let got = install_at_quit(Some(m), |_, _| panic!("nothing is staged"), &launcher);
        assert_eq!(got, QuitInstall::Nothing);
    }
    assert!(launcher.calls.borrow().is_empty(), "something was launched");
}

#[test]
fn quit_without_global_does_nothing() {
    let launcher = FakeLauncher::default();
    let got = install_at_quit(None::<&mut Model>, |_, _| panic!("no updater"), &launcher);
    assert_eq!(got, QuitInstall::Nothing);
    assert!(launcher.calls.borrow().is_empty(), "something was launched");
}

#[test]
fn quit_relaunches_only_on_restart_and_success() {
    for restart in [false, true] {
        let mut m = staged(appimage(), Format::AppImage, restart);
        let launcher = FakeLauncher::default();
        let got = install_at_quit(Some(&mut m), |_, _| restarts(), &launcher);
        assert_eq!(got, QuitInstall::Installed);
        let want = if restart {
            vec![Call::Restart(PathBuf::from(TARGET))]
        } else {
            vec![]
        };
        assert_eq!(*launcher.calls.borrow(), want, "restart = {restart}");
    }
}

#[test]
fn quit_install_failure_launches_nothing() {
    let mut m = staged(appimage(), Format::AppImage, true);
    let launcher = FakeLauncher::default();
    let got = install_at_quit(
        Some(&mut m),
        |_, _| Err(InstallError::HashMismatch),
        &launcher,
    );
    assert_eq!(
        got,
        QuitInstall::InstallFailed(InstallError::HashMismatch.to_string())
    );
    assert!(
        launcher.calls.borrow().is_empty(),
        "a failed install launched"
    );
}

#[test]
fn a_failed_relaunch_is_reported_after_the_install() {
    let mut m = staged(appimage(), Format::AppImage, true);
    let launcher = FakeLauncher {
        fail: true,
        ..FakeLauncher::default()
    };
    let got = install_at_quit(Some(&mut m), |_, _| restarts(), &launcher);
    assert!(matches!(got, QuitInstall::RelaunchFailed(_)), "{got:?}");
    assert_eq!(
        *launcher.calls.borrow(),
        vec![Call::Restart(PathBuf::from(TARGET))]
    );
}

#[test]
fn nsis_restart_appends_r_plain_quit_does_not() {
    let program = PathBuf::from(r"C:\Temp\placemat-update-1.2.0-setup.exe");
    for (restart, want) in [(false, vec!["/P"]), (true, vec!["/P", "/R"])] {
        let mut m = staged(InstallKind::WindowsNsis, Format::Nsis, restart);
        let launcher = FakeLauncher::default();
        let p = program.clone();
        let install = move |_: &InstallKind, _: &str| {
            Ok(Relaunch::Installer {
                program: p,
                args: vec![OsString::from("/P")],
            })
        };
        assert_eq!(
            install_at_quit(Some(&mut m), install, &launcher),
            QuitInstall::Installed
        );
        let want = want.into_iter().map(OsString::from).collect();
        let calls = vec![Call::Installer(program.clone(), want)];
        assert_eq!(*launcher.calls.borrow(), calls, "restart = {restart}");
    }
}

#[test]
fn relaunch_command_waits_for_pid_then_runs() {
    let path = Path::new(TARGET);
    for (os, start) in [
        (Os::Linux, "exec \"$1\""),
        (Os::Macos, "exec /usr/bin/open \"$1\""),
    ] {
        let c = relaunch_command(path, 4242, os).expect("a Unix relaunch");
        assert_eq!(c.get_program(), "/bin/sh");
        let args: Vec<_> = c.get_args().collect();
        assert_eq!(args.len(), 4, "{args:?}");
        assert_eq!(args[0], "-c");
        let script = args[1].to_str().unwrap();
        assert!(script.starts_with("while kill -0 \"$0\""), "{script}");
        assert!(script.ends_with(start), "{script}");
        assert_eq!(args[2], "4242");
        assert_eq!(args[3], path.as_os_str());
    }
    assert!(
        relaunch_command(path, 4242, Os::Windows).is_none(),
        "NSIS relaunches on Windows"
    );
}

#[cfg(unix)]
#[test]
fn relaunch_command_runs_the_target_once_the_pid_is_gone() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("started");
    let target = dir.path().join("fake-app");
    std::fs::write(
        &target,
        format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut gone = std::process::Command::new("true").spawn().unwrap();
    let pid = gone.id();
    gone.wait().unwrap();
    let status = relaunch_command(&target, pid, Os::Linux)
        .unwrap()
        .status()
        .unwrap();
    assert!(status.success(), "{status}");
    assert!(marker.exists(), "the target never ran");
}
