//! D-43 General and appearance: theme, starting with Windows, staying in the
//! tray, the global shortcut and the list of keyboard shortcuts (D-51).
//! Changes save as they are made.
//!
//! Not here yet: website icons (they need a server proxy), language (one
//! language for now) and updates (phase 10).

use gpui_kit::{
    Context, IntoElement, ParentElement, Render, SharedString, Styled, Window, component::v_flex,
    prelude::FluentBuilder as _, px,
};
use kandado_app::{platform, settings::ThemeChoice};

use super::{
    super::{Services, error_line, explain, system::shortcuts_dialog},
    rows::{SettingRow, group_label},
};
use crate::{
    app::Hotkey,
    assets::IconName,
    components::{FieldGroup, Segmented, Toggle, kbd, window_layer},
    theme,
};

const THEMES: [(ThemeChoice, &str); 3] = [
    (ThemeChoice::System, "System"),
    (ThemeChoice::Dark, "Dark"),
    (ThemeChoice::Light, "Light"),
];

pub struct GeneralPage {
    services: Services,
    shortcuts_open: bool,
    failure: Option<SharedString>,
}

impl GeneralPage {
    pub fn new(services: Services) -> Self {
        Self {
            services,
            shortcuts_open: false,
            failure: None,
        }
    }

    fn save(
        &mut self,
        change: impl FnOnce(&mut kandado_app::settings::Settings),
        cx: &mut Context<Self>,
    ) {
        self.failure = self
            .services
            .app
            .update_settings(change)
            .err()
            .map(|error| explain(&error));
        cx.notify();
    }

    pub fn set_theme(&mut self, choice: ThemeChoice, window: &mut Window, cx: &mut Context<Self>) {
        self.save(|settings| settings.theme = choice, cx);
        let kind = theme::kind_for(choice, cx);
        theme::apply(kind, Some(window), cx);
    }

    fn set_start_at_login(&mut self, on: bool, cx: &mut Context<Self>) {
        match platform::set_start_at_login(on) {
            Ok(()) => self.save(|settings| settings.start_at_login = on, cx),
            Err(_) => {
                self.failure = Some("Windows did not accept the change. Try again.".into());
                cx.notify();
            }
        }
    }

    fn set_global_shortcut(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(hotkey) = cx.try_global::<Hotkey>() {
            hotkey.0.set_enabled(on);
        }
        self.save(|settings| settings.global_shortcut = on, cx);
    }

    pub fn open_shortcuts(&mut self, cx: &mut Context<Self>) {
        self.shortcuts_open = true;
        cx.notify();
    }
}

impl Render for GeneralPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = self.services.app.settings();
        let view = cx.entity();
        let selected = THEMES
            .iter()
            .position(|(choice, _)| *choice == settings.theme)
            .unwrap_or(0);
        let page = v_flex()
            .gap(px(8.))
            .child(group_label("Theme", cx))
            .child(
                Segmented::new("theme", THEMES.map(|(_, label)| label), selected).on_select(
                    move |index, window, cx| {
                        view.update(cx, |this, cx| this.set_theme(THEMES[index].0, window, cx));
                    },
                ),
            )
            .child(group_label("App", cx))
            .child(
                FieldGroup::new()
                    .child(
                        SettingRow::new(
                            "row-start-at-login",
                            IconName::Bolt,
                            "Start Kandado when I sign in to my computer",
                        )
                        .trailing(
                            Toggle::new("toggle-start-at-login")
                                .checked(settings.start_at_login)
                                .on_change(cx.listener(|this, on: &bool, _, cx| {
                                    this.set_start_at_login(*on, cx)
                                })),
                        ),
                    )
                    .child(
                        SettingRow::new(
                            "row-keep-in-tray",
                            IconName::Grid,
                            "Keep running in the system tray when closed",
                        )
                        .trailing(
                            Toggle::new("toggle-keep-in-tray")
                                .checked(settings.keep_in_tray)
                                .on_change(cx.listener(|this, on: &bool, _, cx| {
                                    let on = *on;
                                    this.save(|settings| settings.keep_in_tray = on, cx)
                                })),
                        ),
                    ),
            )
            .child(group_label("Shortcuts", cx))
            .child(
                FieldGroup::new()
                    .child(
                        SettingRow::new(
                            "row-global-shortcut",
                            IconName::Key,
                            "Global shortcut to open Kandado",
                        )
                        .description("Ctrl Shift Space, from any app.")
                        .trailing(
                            Toggle::new("toggle-global-shortcut")
                                .checked(settings.global_shortcut)
                                .on_change(cx.listener(|this, on: &bool, _, cx| {
                                    this.set_global_shortcut(*on, cx)
                                })),
                        ),
                    )
                    .child(
                        SettingRow::new(
                            "row-all-shortcuts",
                            IconName::Note,
                            "All keyboard shortcuts",
                        )
                        .trailing(kbd("Ctrl /", cx))
                        .opens(cx.listener(|this, _, _, cx| this.open_shortcuts(cx))),
                    ),
            )
            .when_some(self.failure.clone(), |page, message| {
                page.child(error_line(message, cx))
            });
        let dialog = self.shortcuts_open.then(|| {
            window_layer(
                shortcuts_dialog(
                    cx.listener(|this, _, _, cx| {
                        this.shortcuts_open = false;
                        cx.notify();
                    }),
                    cx,
                ),
                window,
            )
        });
        page.when_some(dialog, |page, dialog| page.child(dialog))
    }
}
