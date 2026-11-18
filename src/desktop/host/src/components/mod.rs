//! The design system's components, in GPUI. Each has the name and the props
//! of its counterpart in `docs/design-system/project/components`, and every
//! colour and size comes from the tokens.
//!
//! `cargo run -p kandado --example gallery` shows them all in both themes.
//!
//! Dialogs, popovers and dropdown menus are GPUI Kit's own, restyled by
//! [`crate::theme::apply`]; the gallery shows those too.

mod badge;
mod button;
mod chip;
mod dialog;
mod field;
mod generator;
mod icon;
mod item;
mod layout;
mod link;
mod logo;
mod meter;
mod surface;
mod toggle;

pub use badge::{Badge, Tone};
pub use button::{Button, ButtonSize, ButtonVariant, IconButton, IconButtonVariant};
pub use chip::VaultChip;
pub use dialog::Dialog;
pub use field::{
    FieldGroup, GROUP_ROW, MASK, SearchBar, SecretField, TextField, TotpCode, group_row_hover,
};
pub use generator::{GeneratorEvent, PasswordGenerator};
pub use icon::Icon;
pub use item::{Hue, ItemIcon, ItemRow};
pub use kandado_model::ItemKind;
pub use layout::{
    EmptyState, FilterChip, ProgressBar, Segmented, TextArea, check, drop_zone, kbd, radio,
    section_label, window_layer,
};
pub use link::Link;
pub use logo::Logo;
pub use meter::StrengthMeter;
pub use surface::{Banner, NavItem, Table, Toast};
pub use toggle::Toggle;

pub use crate::assets::IconName;
