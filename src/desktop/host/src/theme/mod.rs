//! Kandado's two themes, Enamel (light, the default) and Graphite (dark), from
//! the Kandado Hardware design tokens.
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
    /// Light, and the fallback when the system's preference is unknown.
    #[default]
    Enamel,
    /// Dark.
    Graphite,
}

impl ThemeKind {
    pub fn palette(self) -> &'static Palette {
        match self {
            Self::Graphite => &tokens::DARK,
            Self::Enamel => &tokens::LIGHT,
        }
    }

    pub fn other(self) -> Self {
        match self {
            Self::Enamel => Self::Graphite,
            Self::Graphite => Self::Enamel,
        }
    }

    /// Index into the `[dark, light]` pairs in [`tokens`], such as shadows.
    pub fn index(self) -> usize {
        match self {
            Self::Graphite => 0,
            Self::Enamel => 1,
        }
    }
}

struct ActiveTheme(ThemeKind);

impl Global for ActiveTheme {}

/// A token colour (0xRRGGBBAA) as a GPUI colour.
pub fn color(token: u32) -> Hsla {
    rgba(token).into()
}

/// The font family a text token names. Windows lists both bundled variable
/// fonts under the family names the tokens use, so no alias is needed.
pub fn family(family: tokens::Family) -> &'static str {
    match family {
        tokens::Family::Sans => tokens::FONT_SANS,
        tokens::Family::Mono => tokens::FONT_MONO,
    }
}

/// Sets text in one of the design system's type styles: `.text_token(tokens::BODY)`.
///
/// Letter spacing is not applied: GPUI has no letter-spacing property. It
/// matters most for the engraved styles (label, segment, tag), which are spaced
/// 0.06 to 0.14 em.
pub trait TextStyles: Styled + Sized {
    fn text_token(self, token: tokens::TextToken) -> Self {
        self.font_family(family(token.family))
            .text_size(px(token.size))
            .line_height(px(token.line_height))
            .font_weight(FontWeight(f32::from(token.weight)))
    }
}

impl<T: Styled> TextStyles for T {}

/// The keyboard focus ring: 2 px of `focus`, 2 px away from the control.
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
    vec![band(palette.focus, 4.), band(gap, 2.)]
}

/// A token shadow for the current theme, such as `tokens::SHADOW_POP`, with
/// every layer it has (a lit edge, a lip, an inner shadow).
pub fn shadow(token: [&[tokens::ShadowToken]; 2], kind: ThemeKind) -> Vec<BoxShadow> {
    token[kind.index()]
        .iter()
        .map(|layer| BoxShadow {
            color: color(layer.color),
            offset: point(px(layer.x), px(layer.y)),
            blur_radius: px(layer.blur),
            spread_radius: px(layer.spread),
            inset: layer.inset,
        })
        .collect()
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
        ThemeChoice::Dark => ThemeKind::Graphite,
        ThemeChoice::Light => ThemeKind::Enamel,
        ThemeChoice::System => match cx.window_appearance() {
            gpui_kit::WindowAppearance::Dark | gpui_kit::WindowAppearance::VibrantDark => {
                ThemeKind::Graphite
            }
            _ => ThemeKind::Enamel,
        },
    }
}

/// Switches the whole app to `kind`. Call once at startup, after
/// `gpui_kit::init`, and again whenever the person changes the theme.
pub fn apply(kind: ThemeKind, window: Option<&mut Window>, cx: &mut App) {
    cx.set_global(ActiveTheme(kind));
    let mode = match kind {
        ThemeKind::Graphite => ThemeMode::Dark,
        ThemeKind::Enamel => ThemeMode::Light,
    };
    // Resets the component theme to its defaults for the mode; ours go on top.
    Theme::change(mode, window, cx);

    let p = kind.palette();
    let theme = Theme::global_mut(cx);
    theme.radius = px(tokens::RADIUS_KEY);
    theme.radius_lg = px(tokens::RADIUS_PANEL);
    theme.font_size = px(tokens::BODY.size);
    theme.font_family = tokens::FONT_SANS.into();
    theme.mono_font_family = tokens::FONT_MONO.into();

    let c = &mut theme.colors;
    c.background = color(p.body);
    c.foreground = color(p.ink);
    c.border = color(p.edge);
    c.input = color(p.edge_key);
    c.ring = color(p.focus);
    c.caret = color(p.ink);
    c.selection = color(p.raised);
    c.link = color(p.green);
    c.link_hover = color(p.ink);
    c.link_active = color(p.ink);
    c.overlay = color(p.scrim);

    c.muted = color(p.well);
    c.muted_foreground = color(p.ink_2);
    c.accent = color(p.raised);
    c.accent_foreground = color(p.ink);

    // The go key. Hardware keys do not change colour on hover; pressing moves them.
    c.primary = color(p.green);
    c.primary_hover = color(p.green);
    c.primary_active = color(p.green);
    c.primary_foreground = color(p.on_green);
    c.button_primary = color(p.green);
    c.button_primary_hover = color(p.green);
    c.button_primary_active = color(p.green);
    c.button_primary_foreground = color(p.on_green);

    c.secondary = color(p.raised);
    c.secondary_hover = color(p.raised);
    c.secondary_active = color(p.raised);
    c.secondary_foreground = color(p.ink);

    c.danger = color(p.danger);
    c.danger_hover = color(p.danger);
    c.danger_active = color(p.danger);
    c.danger_foreground = color(p.on_danger);
    c.success = color(p.led_ok);
    c.success_foreground = color(p.ink);
    c.warning = color(p.led_warn);

    c.popover = color(p.panel);
    c.popover_foreground = color(p.ink);
    c.sidebar = color(p.panel);
    c.sidebar_foreground = color(p.ink);
    c.sidebar_border = color(p.edge);
    c.title_bar = color(p.body);
    c.title_bar_border = color(p.edge);
    c.list = color(p.well);
    c.list_hover = color(p.well);
    c.list_active = color(p.raised);
    c.switch = color(p.well);
    c.slider_bar = color(p.green);
    c.slider_thumb = color(p.raised);
    c.progress_bar = color(p.led_ok);
    c.scrollbar_thumb = color(p.tag);
    c.scrollbar_thumb_hover = color(p.ink_2);

    Theme::sync_base(cx);
    cx.refresh_windows();
}

#[cfg(test)]
mod tests {
    use super::{tokens::*, *};

    #[test]
    fn colours_come_out_of_the_tokens_file_unchanged() {
        assert_eq!(LIGHT.body, 0xe4e0d8ff);
        assert_eq!(DARK.body, 0x121517ff);
        assert_eq!(LIGHT.green, 0x1f5a4aff);
        assert_eq!(DARK.on_green, 0x07150fff);
    }

    #[test]
    fn aliases_resolve_per_theme() {
        assert_eq!(DARK.focus, DARK.green);
        assert_eq!(LIGHT.focus, LIGHT.green);
    }

    #[test]
    fn the_lcd_is_dark_in_both_themes() {
        assert_eq!(LIGHT.lcd_ink, DARK.lcd_ink);
        assert_eq!(LIGHT.lcd_label, DARK.lcd_label);
        assert_eq!((LIGHT.lcd, DARK.lcd), (0x16201cff, 0x070b09ff));
    }

    #[test]
    fn translucent_colours_keep_their_alpha() {
        // rgba(20, 22, 24, 0.45) and rgba(0, 0, 0, 0.62)
        assert_eq!(LIGHT.scrim, 0x14161873);
        assert_eq!(DARK.scrim, 0x0000009e);
    }

    #[test]
    fn type_spacing_and_radius_tokens() {
        assert_eq!(
            (FONT_SANS, FONT_MONO),
            ("Schibsted Grotesk", "Martian Mono")
        );
        assert_eq!(
            DISPLAY,
            TextToken {
                family: Family::Sans,
                size: 44.0,
                line_height: 44.0,
                weight: 800,
                letter_spacing: -0.02
            }
        );
        assert_eq!(
            LABEL,
            TextToken {
                family: Family::Mono,
                size: 10.0,
                line_height: 14.0,
                weight: 500,
                letter_spacing: 0.14
            }
        );
        assert_eq!(LCD_CODE.family, Family::Mono);
        assert_eq!(
            (SPACE_1, SPACE_4, SPACE_5, SPACE_7),
            (4.0, 14.0, 16.0, 32.0)
        );
        assert_eq!(
            (RADIUS_TAG, RADIUS_KEY, RADIUS_PANEL, RADIUS_ROUND),
            (5.0, 12.0, 18.0, 999.0)
        );
    }

    #[test]
    fn shadows_keep_every_layer_and_inner_shadows() {
        // "inset 0 1px 0 rgba(255, 255, 255, 0.9), 0 2px 0 #CFC9BD"
        assert_eq!(
            SHADOW_PANEL[ThemeKind::Enamel.index()],
            &[
                ShadowToken {
                    x: 0.0,
                    y: 1.0,
                    blur: 0.0,
                    spread: 0.0,
                    color: 0xffffffe6,
                    inset: true
                },
                ShadowToken {
                    x: 0.0,
                    y: 2.0,
                    blur: 0.0,
                    spread: 0.0,
                    color: 0xcfc9bdff,
                    inset: false
                },
            ]
        );
        // One value for both themes.
        assert_eq!(SHADOW_LCD[0], SHADOW_LCD[1]);
        assert!(SHADOW_LCD[0][0].inset);
        assert_eq!(SHADOW_POP[ThemeKind::Graphite.index()].len(), 2);
        assert_eq!(shadow(SHADOW_KEY, ThemeKind::Enamel).len(), 1);
    }

    #[test]
    fn the_two_themes_are_each_others_opposite() {
        assert_eq!(ThemeKind::default(), ThemeKind::Enamel);
        assert_eq!(ThemeKind::Graphite.other(), ThemeKind::Enamel);
        assert_eq!(ThemeKind::Enamel.other(), ThemeKind::Graphite);
        assert_eq!(ThemeKind::Graphite.palette(), &DARK);
        assert_eq!(ThemeKind::Enamel.palette(), &LIGHT);
    }
}
