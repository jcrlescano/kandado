//! Dialogs and banners that belong to no single screen.

mod shortcuts;
mod sign_out;

pub use shortcuts::{SHORTCUTS, shortcuts_dialog};
pub use sign_out::sign_out_dialog;
