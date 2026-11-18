//! D-51 Keyboard shortcuts: the list, opened with Ctrl / or from Settings ›
//! General. The keys themselves are handled by the vault shell.

use gpui_kit::{App, ClickEvent, ParentElement as _, Styled, Window, component::h_flex, div, px};

use crate::{
    components::{Button, Dialog, FieldGroup, kbd},
    theme::{KandadoTheme, TextStyles as _, color, tokens},
};

/// Every shortcut, as the dialog lists them.
pub const SHORTCUTS: [(&str, &str); 11] = [
    ("Search", "Ctrl F"),
    ("Create login", "Ctrl N"),
    ("Copy username", "Ctrl B"),
    ("Copy password", "Ctrl C"),
    ("Copy two-factor code", "Ctrl T"),
    ("Edit item", "Ctrl E"),
    ("Save", "Ctrl S"),
    ("Lock", "Ctrl L"),
    ("Move to trash", "Delete"),
    ("Next / previous item", "↓ / ↑"),
    ("Open Kandado from anywhere", "Ctrl Shift Space"),
];

pub fn shortcuts_dialog(
    on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Dialog {
    let p = cx.palette();
    let mut group = FieldGroup::new();
    for (label, keys) in SHORTCUTS {
        group = group.child(
            h_flex()
                .items_center()
                .min_h(px(44.))
                .px(px(tokens::SPACE_4))
                .child(
                    div()
                        .flex_1()
                        .text_token(tokens::BODY)
                        .text_color(color(p.ink))
                        .child(label),
                )
                .child(kbd(keys, cx)),
        );
    }
    Dialog::new("Keyboard shortcuts")
        .wide()
        .child(group)
        .action(Button::new("shortcuts-close", "Close").on_click(on_close))
}
