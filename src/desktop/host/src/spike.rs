//! The phase 0 GPUI spike: one window that exercises everything the desktop
//! design depends on, so the framework is proven before feature work starts.
//!
//! It is throwaway UI with real mechanics: a custom title bar with a centred
//! search field, the three-pane layout with the sidebar collapsing to a rail
//! below 1120 px, a 5,000-row virtual list, text input (including IME
//! composition), a masked field, a dialog, a popover, a toast and both themes.
//!
//! Run it with `cargo run -p kandado --example spike`. The tests at the
//! bottom drive the same view headlessly.

use std::ops::Range;

use gpui_kit::{
    component::{
        ActiveTheme, Sizable, TitleBar, WindowExt,
        button::{Button, ButtonVariants},
        h_flex,
        input::{Input, InputState},
        notification::Notification,
        popover::Popover,
        v_flex,
    },
    prelude::*,
    *,
};

use crate::theme::{self, KandadoTheme};

/// Below this window width the sidebar becomes a rail (D-17).
pub const RAIL_BREAKPOINT: Pixels = px(1120.);
pub const SIDEBAR_WIDTH: Pixels = px(240.);
pub const RAIL_WIDTH: Pixels = px(64.);
pub const LIST_WIDTH: Pixels = px(360.);
pub const ROW_COUNT: usize = 5_000;
const ROW_HEIGHT: Pixels = px(56.);
/// Three 35 px window controls on the right, less the title bar's own 12 px left padding.
const TITLE_BAR_LEFT_INSET: Pixels = px(3. * 35. - 12.);

pub struct Spike {
    search: Entity<InputState>,
    title: Entity<InputState>,
    password: Entity<InputState>,
    selected: usize,
}

impl Spike {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            search: cx.new(|cx| InputState::new(window, cx).placeholder("Search in all vaults")),
            title: cx.new(|cx| InputState::new(window, cx).default_value("GitHub")),
            password: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .default_value("correct horse battery")
            }),
            selected: 0,
        }
    }

    pub fn password(&self) -> &Entity<InputState> {
        &self.password
    }

    pub fn title(&self) -> &Entity<InputState> {
        &self.title
    }

    fn sidebar(&self, rail: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let entries = [
            "All items",
            "Personal",
            "Family",
            "Work",
            "Monitor",
            "Trash",
        ];
        v_flex()
            .id("sidebar")
            .test_support()
            .w(if rail { RAIL_WIDTH } else { SIDEBAR_WIDTH })
            .flex_none()
            .h_full()
            .gap_1()
            .p_2()
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .children(entries.into_iter().map(|entry| {
                let initial: SharedString = entry[..1].to_owned().into();
                h_flex()
                    .h_9()
                    .px_3()
                    .gap_3()
                    .rounded_md()
                    .when(rail, |row| row.justify_center().px_0())
                    .child(
                        div()
                            .size_6()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_md()
                            .bg(cx.theme().muted)
                            .text_xs()
                            .child(initial),
                    )
                    .when(!rail, |row| row.child(entry))
            }))
    }

    fn rows(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<impl IntoElement + use<>> {
        range
            .map(|index| {
                let selected = index == self.selected;
                v_flex()
                    .id(("row", index))
                    .test_support()
                    .w_full()
                    .h(ROW_HEIGHT)
                    .px_4()
                    .justify_center()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .when(selected, |row| row.bg(cx.theme().accent))
                    .hover(|row| row.bg(cx.theme().muted))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected = index;
                        cx.notify();
                    }))
                    .child(format!("Login {}", index + 1))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("user{}@example.com", index + 1)),
                    )
            })
            .collect()
    }

    fn list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("item-list")
            .test_support()
            .w(LIST_WIDTH)
            .flex_none()
            .h_full()
            .border_r_1()
            .border_color(cx.theme().border)
            .child(uniform_list("rows", ROW_COUNT, cx.processor(Self::rows)).size_full())
    }

    fn detail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let dark = cx.theme().is_dark();
        v_flex()
            .id("detail")
            .test_support()
            .flex_1()
            .min_w_0()
            .h_full()
            .p_6()
            .gap_4()
            .child(
                div()
                    .text_xl()
                    .child(format!("Login {}", self.selected + 1)),
            )
            .child(Input::new(&self.title).id("title").w(px(320.)))
            .child(
                Input::new(&self.password)
                    .id("password")
                    .mask_toggle()
                    .w(px(320.)),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(Button::new("open-dialog").label("Delete forever").on_click(
                        |_, window, cx| {
                            window.open_dialog(cx, |dialog, _, _| {
                                dialog
                                    .title("Delete this item forever?")
                                    .child("This cannot be undone.")
                            });
                        },
                    ))
                    .child(
                        Popover::new("generator")
                            .trigger(Button::new("open-popover").label("Generate"))
                            .content(|_, _, _| {
                                div()
                                    .id("popover-body")
                                    .test_support()
                                    .p_4()
                                    .w(px(360.))
                                    .child("Password generator")
                            }),
                    )
                    .child(
                        Button::new("show-toast")
                            .primary()
                            .label("Copy password")
                            .on_click(|_, window, cx| {
                                window.push_notification(
                                    Notification::new().message("Password copied"),
                                    cx,
                                );
                            }),
                    )
                    .child(
                        Button::new("toggle-theme")
                            .small()
                            .label(if dark { "Paper theme" } else { "Vault theme" })
                            .on_click(|_, window, cx| {
                                let kind = cx.theme_kind().other();
                                theme::apply(kind, Some(window), cx);
                            }),
                    ),
            )
    }
}

impl Render for Spike {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rail = window.viewport_size().width < RAIL_BREAKPOINT;
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                TitleBar::new().child(
                    h_flex()
                        .flex_1()
                        // Balances the window controls on the right, so the
                        // search field is centred in the window, not in the gap.
                        .pl(TITLE_BAR_LEFT_INSET)
                        .justify_center()
                        .child(Input::new(&self.search).id("search").small().w(px(420.))),
                ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar(rail, cx))
                    .child(self.list(cx))
                    .child(self.detail(cx)),
            )
    }
}

/// Window options for the spike: client-side title bar, sensible minimum size.
pub fn window_options(cx: &App) -> WindowOptions {
    let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(720.), px(480.))),
        ..TitleBar::window_options()
    }
}
