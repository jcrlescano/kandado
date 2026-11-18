//! D-52 Sign out dialog. Signing out removes the local vault from this
//! computer; the vault itself stays safe in the person's account.

use gpui_kit::{App, ClickEvent, ParentElement as _, SharedString, Window};

use crate::{
    assets::IconName,
    components::{Button, ButtonVariant, Dialog, Tone},
};

type Handler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// The confirmation. `on_confirm` signs out; `on_cancel` closes the dialog.
/// While `busy` both buttons are disabled; `failure` shows under the text.
pub fn sign_out_dialog(
    busy: bool,
    failure: Option<SharedString>,
    on_cancel: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_confirm: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Dialog {
    let on_cancel: Handler = Box::new(on_cancel);
    let on_confirm: Handler = Box::new(on_confirm);
    let mut dialog = Dialog::new("Sign out of Kandado?")
        .icon(IconName::Logout, Tone::Danger)
        .body(
            "Your vault stays safe in your account. This computer forgets it until you sign in again.",
        );
    if let Some(failure) = failure {
        dialog = dialog.child(crate::views::error_line(failure, cx));
    }
    dialog
        .action(
            Button::new("sign-out-cancel", "Cancel")
                .disabled(busy)
                .on_click(move |event, window, cx| on_cancel(event, window, cx)),
        )
        .action(
            Button::new(
                "sign-out-confirm",
                if busy { "Signing out…" } else { "Sign out" },
            )
            .variant(ButtonVariant::Danger)
            .disabled(busy)
            .on_click(move |event, window, cx| on_confirm(event, window, cx)),
        )
}
