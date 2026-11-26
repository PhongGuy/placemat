//! The main window's content: the title bar over the board editor, or over a centred empty
//! state offering to open a board while none is open.

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, TitleBar};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, ParentElement as _, PathPromptOptions, Render, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _, Window, div,
};

use crate::theme;
use crate::viewer::OpenBoard;
use crate::viewer::editor::BoardEditor;
use crate::viewer::measure::GpuiMeasure;
use crate::viewer::notice::{Tickets, clean, error_text};
use crate::viewer::open::open_board;

/// The window's only view, which also keeps the theme on the OS appearance.
pub struct MainView {
    board: Option<Entity<BoardEditor>>,
    error: Option<SharedString>,
    focus: FocusHandle,
    tickets: Tickets,
    _appearance: Subscription,
}

impl MainView {
    /// The view for `window`, in the mode the OS asks for now and following its changes. It
    /// takes the window's focus so the open key works before any click. With a `path`, the
    /// board opens as `open` would open it.
    pub fn new(window: &mut Window, path: Option<PathBuf>, cx: &mut Context<Self>) -> MainView {
        theme::follow(window.appearance(), window, cx);
        let appearance = cx.observe_window_appearance(window, |_, window, cx| {
            theme::follow(window.appearance(), window, cx);
        });
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let mut view = MainView {
            board: None,
            error: None,
            focus,
            tickets: Tickets::default(),
            _appearance: appearance,
        };
        if let Some(path) = path {
            view.open(path, cx);
        }
        view
    }

    /// Opens the board at `path` in the background and shows it once it loads, unless a later
    /// open has started meanwhile. A board that fails to open keeps the shown one and says why
    /// in the error banner; one that opens clears it.
    fn open(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let ticket = self.tickets.issue();
        let measure = Arc::new(GpuiMeasure::new(Arc::clone(cx.text_system())));
        cx.spawn(async move |this, cx| {
            let opened = cx
                .background_executor()
                .spawn({
                    let path = path.clone();
                    async move { open_board(path, measure) }
                })
                .await;
            // The window may have closed while the board loaded; then there is nothing to
            // show it in.
            let _ = this.update(cx, |view, cx| {
                if !view.tickets.is_latest(ticket) {
                    return;
                }
                match opened {
                    Ok(opened) => {
                        view.board = Some(cx.new(|cx| BoardEditor::new(opened, cx)));
                        view.error = None;
                    }
                    Err(e) => view.error = Some(error_text(&path, &e).into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Asks for one board file through the system dialog and opens it. A cancel or a dropped
    /// reply does nothing; a dialog the platform cannot show is reported in the error banner.
    fn prompt_open(&mut self, cx: &mut Context<Self>) {
        let reply = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open board".into()),
        });
        cx.spawn(async move |this, cx| {
            let path = match reply.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Err(e)) => {
                    let text = clean(&format!("Could not show the file dialog: {e}"));
                    let _ = this.update(cx, |view, cx| {
                        view.error = Some(text.into());
                        cx.notify();
                    });
                    return;
                }
                Ok(Ok(None)) | Err(_) => None,
            };
            let Some(path) = path else {
                return;
            };
            let _ = this.update(cx, |view, cx| view.open(path, cx));
        })
        .detach();
    }

    /// The banner saying why the last open failed, while it stands.
    fn error_banner(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let text = self.error.clone()?;
        Some(
            div()
                .id("board-error")
                .role(Role::Alert)
                .aria_label(text.clone())
                .test_support()
                .flex_none()
                .px_3()
                .py_1()
                .text_sm()
                .bg(cx.theme().danger)
                .text_color(cx.theme().danger_foreground)
                .child(text)
                .into_any_element(),
        )
    }

    /// The open board's editor, or the empty state offering to open one while there is none.
    fn content(&self, cx: &Context<Self>) -> AnyElement {
        match &self.board {
            Some(editor) => editor.clone().into_any_element(),
            None => div()
                .id("placeholder")
                .aria_label("Placemat")
                .test_support()
                .flex()
                .flex_col()
                .items_center()
                .gap_4()
                .child(
                    div()
                        .text_2xl()
                        .text_color(cx.theme().muted_foreground)
                        .child("Placemat"),
                )
                .child(
                    Button::new("open-board")
                        .primary()
                        .label("Open board")
                        .on_click(cx.listener(|view, _, _, cx| view.prompt_open(cx))),
                )
                .into_any_element(),
        }
    }
}

impl Render for MainView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .track_focus(&self.focus)
            .key_context("MainView")
            .on_action(cx.listener(|view, _: &OpenBoard, _, cx| view.prompt_open(cx)))
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(TitleBar::new())
            .children(self.error_banner(cx))
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
