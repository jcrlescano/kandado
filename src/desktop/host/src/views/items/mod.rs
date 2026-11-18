//! The item screens drawn in the detail pane: details (D-07, D-09 – D-12),
//! forms (D-19, D-22 – D-24), add two-factor code (D-20) and history (D-16).
//! The list, search, multi-select, create menu and move dialog are drawn by
//! the shell, which owns the list (`crate::shell`).

pub mod add_totp;
pub mod detail;
pub mod form;
pub mod history;
