//! The main window's content: the title bar over the board editor, or over a centred
//! placeholder while no board is open.

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::component::{ActiveTheme as _, TitleBar};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Subscription,
    TestSupportExt as _, Window, div,
};

use crate::theme;
use crate::viewer::editor::BoardEditor;
use crate::viewer::measure::GpuiMeasure;
use crate::viewer::open::open_board;

/// The window's only view, which also keeps the theme on the OS appearance.
pub struct MainView {
    board: Option<Entity<BoardEditor>>,
    _appearance: Subscription,
}

impl MainView {
    /// The view for `window`, in the mode the OS asks for now and following its changes. With
    /// a `path`, the board opens in the background and replaces the placeholder once it loads;
    /// a board that fails to open is reported on stderr and the placeholder stays.
    pub fn new(window: &mut Window, path: Option<PathBuf>, cx: &mut Context<Self>) -> MainView {
        theme::follow(window.appearance(), window, cx);
        let appearance = cx.observe_window_appearance(window, |_, window, cx| {
            theme::follow(window.appearance(), window, cx);
        });
        if let Some(path) = path {
            let measure = Arc::new(GpuiMeasure::new(Arc::clone(cx.text_system())));
            cx.spawn(async move |this, cx| {
                let shown = path.display().to_string();
                let opened = cx
                    .background_executor()
                    .spawn(async move { open_board(path, measure) })
                    .await;
                match opened {
                    Ok(opened) => {
                        // The window may have closed while the board loaded; then there is
                        // nothing to show it in.
                        let _ = this.update(cx, |view, cx| {
                            view.board = Some(cx.new(|cx| BoardEditor::new(opened, cx)));
                            cx.notify();
                        });
                    }
                    Err(e) => eprintln!("placemat: {shown}: {e}"),
                }
            })
            .detach();
        }
        MainView {
            board: None,
            _appearance: appearance,
        }
    }

    /// The open board's editor, or the placeholder while there is none.
    fn content(&self, cx: &Context<Self>) -> AnyElement {
        match &self.board {
            Some(editor) => editor.clone().into_any_element(),
            None => div()
                .id("placeholder")
                .aria_label("Placemat")
                .test_support()
                .text_2xl()
                .text_color(cx.theme().muted_foreground)
                .child("Placemat")
                .into_any_element(),
        }
    }
}

impl Render for MainView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(TitleBar::new())
            .child(
                div()
                    .id("main")
                    .test_support()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(self.content(cx)),
            )
    }
}
