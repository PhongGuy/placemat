//! The main window's content: the title bar over an empty area with a centred placeholder.

use gpui_kit::component::{ActiveTheme as _, TitleBar};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _, Window, div,
};

use crate::theme;

/// The window's only view, which also keeps the theme on the OS appearance.
pub struct MainView {
    _appearance: Subscription,
}

impl MainView {
    /// The view for `window`, in the mode the OS asks for now and following its changes.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> MainView {
        theme::follow(window.appearance(), window, cx);
        let appearance = cx.observe_window_appearance(window, |_, window, cx| {
            theme::follow(window.appearance(), window, cx);
        });
        MainView {
            _appearance: appearance,
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
                    .child(
                        div()
                            .id("placeholder")
                            .aria_label("Placemat")
                            .test_support()
                            .text_2xl()
                            .text_color(theme.muted_foreground)
                            .child("Placemat"),
                    ),
            )
    }
}
