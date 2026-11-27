//! The Placemat desktop app.

// A release build links as a Windows GUI program so no console opens beside the window;
// debug builds keep the console for their logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use placemat_app::launch::{self, Launch};

fn main() {
    // The command line is settled before GPUI starts, so help, version and a bad argument all
    // answer without a display.
    let path = match launch::parse(std::env::args_os().skip(1)) {
        Ok(Launch::Help) => return println!("{}", launch::USAGE),
        Ok(Launch::Version) => return println!("placemat {}", env!("CARGO_PKG_VERSION")),
        Ok(Launch::Open { path }) => path,
        // A launch kind this binary does not know yet opens the empty window.
        Ok(_) => None,
        Err(e) => {
            eprintln!("placemat: {e}\n\n{}", launch::USAGE);
            std::process::exit(2);
        }
    };
    // GPUI waits forever for a Linux display that is not there, so say so and stop.
    #[cfg(target_os = "linux")]
    if !launch::has_display(|v| std::env::var_os(v)) {
        eprintln!("placemat needs a graphical display (WAYLAND_DISPLAY or DISPLAY is not set)");
        std::process::exit(1);
    }
    gpui_kit::application()
        .with_assets(placemat_app::ui::AppAssets)
        .run(|cx| {
            gpui_kit::init(cx);
            placemat_app::theme::install(cx);
            placemat_app::viewer::init(cx);
            placemat_app::app::init(cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            match placemat_app::app::start(cx, path) {
                Ok(_) => cx.activate(true),
                Err(e) => {
                    eprintln!("placemat: {e}");
                    cx.quit();
                }
            }
        });
}
