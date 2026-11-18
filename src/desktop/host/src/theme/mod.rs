//! Kandado's two themes, Vault (dark) and Paper (light), from the design tokens.
//!
//! Every colour and size in the app comes from [`tokens`], which `build.rs`
//! generates from the synced `resources/tokens.json`. Views read the current
//! palette with [`KandadoTheme::palette`]; nothing hard-codes a colour.
//!
//! Themes are only ever switched through [`apply`], which also restyles the
//! GPUI Kit components (inputs, dialogs, popovers) to match.

use gpui_kit::{
    App, BoxShadow, FontWeight, Global, Hsla, Styled, Window,
    component::{Theme, ThemeMode},
    point, px, rgba,
};

pub mod fonts;

/// Generated from `resources/tokens.json`.
pub mod tokens {
    include!(concat!(env!("OUT_DIR"), "/tokens.rs"));
}

use tokens::Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeKind {
    /// Dark.
    #[default]
    Vault,
    /// Light.
    Paper,
}

impl ThemeKind {
    pub fn palette(self) -> &'static Palette {
        match self {
            Self::Vault => &tokens::DARK,
            Self::Paper => &tokens::LIGHT,
        }
    }

    pub fn other(self) -> Self {
        match self {
            Self::Vault => Self::Paper,
            Self::Paper => Self::Vault,
        }
    }

    /// Index into the `[dark, light]` pairs in [`tokens`], such as shadows.
    pub fn index(self) -> usize {
        match self {
            Self::Vault => 0,
            Self::Paper => 1,
        }
    }
}

struct ActiveTheme(ThemeKind);

impl Global for ActiveTheme {}

/// A token colour (0xRRGGBBAA) as a GPUI colour.
pub fn color(token: u32) -> Hsla {
    rgba(token).into()
}

/// The name Windows knows the bundled display font by. The file is a variable
/// font with an optical-size axis, and DirectWrite lists it under its default
/// optical size rather than the plain `tokens::FONT_DISPLAY` name.
pub const DISPLAY_FACE: &str = "Bricolage Grotesque 14pt";

/// The font family a text token names.
pub fn family(family: tokens::Family) -> &'static str {
    match family {
        tokens::Family::Display => DISPLAY_FACE,
        tokens::Family::Sans => tokens::FONT_SANS,
        tokens::Family::Mono => tokens::FONT_MONO,
    }
}

/// Sets text in one of the design system's type styles: `.text_token(tokens::BODY)`.
///
/// Letter spacing is not applied: GPUI has no letter-spacing property. It
/// affects four styles (display-lg, title-lg, label, totp) by at most 0.08 em.
pub trait TextStyles: Styled + Sized {
    fn text_token(self, token: tokens::TextToken) -> Self {
        self.font_family(family(token.family))
            .text_size(px(token.size))
            .line_height(px(token.line_height))
            .font_weight(FontWeight(f32::from(token.weight)))
    }
}

impl<T: Styled> TextStyles for T {}

/// The keyboard focus ring: 2 px of `focus-ring`, 2 px away from the control.
///
/// GPUI has no outline, so this is two hard-edged shadows: the ring, and
/// under it a 2 px band in `gap`, the colour of the surface the control sits on.
pub fn focus_ring(palette: &Palette, gap: u32) -> Vec<BoxShadow> {
    let band = |token, spread: f32| BoxShadow {
        color: color(token),
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(spread),
        inset: false,
    };
    vec![band(palette.focus_ring, 4.), band(gap, 2.)]
}

/// A token shadow for the current theme, such as `tokens::SHADOW_POP`.
pub fn shadow(token: [tokens::ShadowToken; 2], kind: ThemeKind) -> Vec<BoxShadow> {
    let shadow = token[kind.index()];
    vec![BoxShadow {
        color: color(shadow.color),
        offset: point(px(shadow.x), px(shadow.y)),
        blur_radius: px(shadow.blur),
        spread_radius: px(0.),
        inset: false,
    }]
}

/// Read access to the current Kandado theme.
pub trait KandadoTheme {
    fn theme_kind(&self) -> ThemeKind;

    fn palette(&self) -> &'static Palette {
        self.theme_kind().palette()
    }
}

impl KandadoTheme for App {
    fn theme_kind(&self) -> ThemeKind {
        self.try_global::<ActiveTheme>()
            .map(|active| active.0)
            .unwrap_or_default()
    }
}

/// The theme a setting means: "System" follows Windows' light or dark mode.
pub fn kind_for(choice: kandado_app::settings::ThemeChoice, cx: &App) -> ThemeKind {
    use kandado_app::settings::ThemeChoice;
    match choice {
        ThemeChoice::Dark => ThemeKind::Vault,
        ThemeChoice::Light => ThemeKind::Paper,
        ThemeChoice::System => match cx.window_appearance() {
            gpui_kit::WindowAppearance::Light | gpui_kit::WindowAppearance::VibrantLight => {
                ThemeKind::Paper
            }
            _ => ThemeKind::Vault,
        },
    }
}

/// Switches the whole app to `kind`. Call once at startup, after
/// `gpui_kit::init`, and again whenever the person changes the theme.
pub fn apply(kind: ThemeKind, window: Option<&mut Window>, cx: &mut App) {
    cx.set_global(ActiveTheme(kind));
    let mode = match kind {
        ThemeKind::Vault => ThemeMode::Dark,
        ThemeKind::Paper => ThemeMode::Light,
    };
    // Resets the component theme to its defaults for the mode; ours go on top.
    Theme::change(mode, window, cx);

    let p = kind.palette();
    let theme = Theme::global_mut(cx);
    theme.radius = px(tokens::RADIUS_MD);
    theme.radius_lg = px(tokens::RADIUS_LG);
    theme.font_size = px(tokens::BODY.size);
    theme.font_family = tokens::FONT_SANS.into();
    theme.mono_font_family = tokens::FONT_MONO.into();

    let c = &mut theme.colors;
    c.background = color(p.surface_000);
    c.foreground = color(p.ink);
    c.border = color(p.line);
    c.input = color(p.line_strong);
    c.ring = color(p.focus_ring);
    c.caret = color(p.ink);
    c.selection = color(p.brass_soft);
    c.link = color(p.link);
    c.link_hover = color(p.link);
    c.link_active = color(p.link);
    c.overlay = color(p.scrim);

    c.muted = color(p.surface_200);
    c.muted_foreground = color(p.ink_muted);
    c.accent = color(p.surface_300);
    c.accent_foreground = color(p.ink);

    c.primary = color(p.brass);
    c.primary_hover = color(p.brass_strong);
    c.primary_active = color(p.brass_strong);
    c.primary_foreground = color(p.on_brass);
    c.button_primary = color(p.brass);
    c.button_primary_hover = color(p.brass_strong);
    c.button_primary_active = color(p.brass_strong);
    c.button_primary_foreground = color(p.on_brass);

    c.secondary = color(p.surface_200);
    c.secondary_hover = color(p.surface_300);
    c.secondary_active = color(p.surface_300);
    c.secondary_foreground = color(p.ink);

    c.danger = color(p.danger);
    c.danger_hover = color(p.danger);
    c.danger_active = color(p.danger);
    c.danger_foreground = color(p.on_danger);
    c.success = color(p.teal);
    c.success_foreground = color(p.on_teal);
    c.warning = color(p.warning);

    c.popover = color(p.surface_100);
    c.popover_foreground = color(p.ink);
    c.sidebar = color(p.surface_100);
    c.sidebar_foreground = color(p.ink);
    c.sidebar_border = color(p.line);
    c.title_bar = color(p.surface_000);
    c.title_bar_border = color(p.line);
    c.list = color(p.surface_100);
    c.list_hover = color(p.surface_200);
    c.list_active = color(p.surface_300);
    c.switch = color(p.line_strong);
    c.slider_bar = color(p.brass);
    c.slider_thumb = color(p.on_brass);
    c.progress_bar = color(p.teal);
    c.scrollbar_thumb = color(p.line_strong);
    c.scrollbar_thumb_hover = color(p.ink_faint);

    Theme::sync_base(cx);
    cx.refresh_windows();
}

#[cfg(test)]
mod tests {
    use super::{tokens::*, *};

    #[test]
    fn colours_come_out_of_the_tokens_file_unchanged() {
        assert_eq!(DARK.surface_000, 0x0d1214ff);
        assert_eq!(LIGHT.surface_000, 0xf5f3eeff);
        assert_eq!(DARK.brass, 0xe7b85aff);
        assert_eq!(LIGHT.on_brass, 0xffffffff);
    }

    #[test]
    fn aliases_resolve_per_theme() {
        assert_eq!(DARK.focus_ring, DARK.teal);
        assert_eq!(LIGHT.focus_ring, LIGHT.teal);
        assert_eq!(DARK.kind_card, DARK.coral);
        assert_eq!(LIGHT.kind_password, LIGHT.brass);
    }

    #[test]
    fn translucent_colours_keep_their_alpha() {
        // rgba(3, 6, 7, 0.72) and rgba(19, 32, 30, 0.45)
        assert_eq!(DARK.scrim, 0x030607b8);
        assert_eq!(LIGHT.scrim, 0x13201e73);
    }

    #[test]
    fn type_spacing_radius_and_shadow_tokens() {
        assert_eq!(
            (FONT_DISPLAY, FONT_SANS, FONT_MONO),
            ("Bricolage Grotesque", "Figtree", "JetBrains Mono")
        );
        assert_eq!(
            DISPLAY_LG,
            TextToken {
                family: Family::Display,
                size: 40.0,
                line_height: 44.0,
                weight: 700,
                letter_spacing: -0.02
            }
        );
        assert_eq!(
            BODY,
            TextToken {
                family: Family::Sans,
                size: 15.0,
                line_height: 22.0,
                weight: 400,
                letter_spacing: 0.0
            }
        );
        assert_eq!(TOTP.family, Family::Mono);
        assert_eq!((SPACE_1, SPACE_4, SPACE_7), (4.0, 16.0, 48.0));
        assert_eq!((RADIUS_SM, RADIUS_MD, RADIUS_PILL), (6.0, 10.0, 999.0));
        assert_eq!(
            SHADOW_POP[0],
            ShadowToken {
                x: 0.0,
                y: 8.0,
                blur: 24.0,
                color: 0x00000080
            }
        );
        assert_eq!(
            SHADOW_SHEET[1],
            ShadowToken {
                x: 0.0,
                y: -8.0,
                blur: 32.0,
                color: 0x13201e29
            }
        );
    }

    #[test]
    fn the_two_themes_are_each_others_opposite() {
        assert_eq!(ThemeKind::Vault.other(), ThemeKind::Paper);
        assert_eq!(ThemeKind::Paper.other(), ThemeKind::Vault);
        assert_eq!(ThemeKind::Vault.palette(), &DARK);
        assert_eq!(ThemeKind::Paper.palette(), &LIGHT);
    }
}
