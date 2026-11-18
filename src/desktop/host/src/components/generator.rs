use gpui_kit::{
    App, AppContext as _, ClickEvent, Context, ElementId, Entity, EventEmitter, FontWeight,
    HighlightStyle, InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, StyledText, Window,
    component::slider::{Slider, SliderEvent, SliderState},
    div,
    prelude::FluentBuilder as _,
    px,
};
use kandado_model::{
    Secret,
    generator::{self, Generated, MAX_LENGTH, MAX_WORDS, MIN_LENGTH, MIN_WORDS, Options},
    strength,
};

use super::{Button, ButtonSize, IconButton, StrengthMeter, Toggle, hardware::lcd};
use crate::{
    assets::IconName,
    theme::{KandadoTheme, TextStyles as _, color, focus_ring, tokens},
};

/// What the person asked for. The owner acts on it: the generator never
/// touches the clipboard or an item itself.
#[derive(Debug, Clone)]
pub enum GeneratorEvent {
    /// Copy pressed.
    Copy(Secret),
    /// "Use password" pressed, in a create or edit flow.
    Use(Secret),
}

/// The password generator: random or memorable, with live strength.
///
/// Defaults: random, 20 characters, uppercase, numbers and symbols on;
/// memorable defaults to 4 words. The generating itself is
/// `kandado_model::generator`.
pub struct PasswordGenerator {
    memorable: bool,
    /// Uppercase letters, or Capitalise for memorable.
    upper: bool,
    numbers: bool,
    /// Symbols, or hyphens between words for memorable.
    symbols: bool,
    length: Entity<SliderState>,
    words: Entity<SliderState>,
    /// `None` only if the system random source failed.
    current: Option<Generated>,
    show_use: bool,
}

impl EventEmitter<GeneratorEvent> for PasswordGenerator {}

impl PasswordGenerator {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let slider = |min: u8, max: u8, value: u8, cx: &mut Context<Self>| {
            let state = cx.new(|_| {
                SliderState::new()
                    .min(f32::from(min))
                    .max(f32::from(max))
                    .step(1.)
                    .default_value(f32::from(value))
            });
            cx.subscribe(&state, |this, _, event: &SliderEvent, cx| {
                if matches!(event, SliderEvent::Change(_)) {
                    this.regenerate(cx);
                }
            })
            .detach();
            state
        };
        let mut this = Self {
            memorable: false,
            upper: true,
            numbers: true,
            symbols: true,
            length: slider(MIN_LENGTH, MAX_LENGTH, 20, cx),
            words: slider(MIN_WORDS, MAX_WORDS, 4, cx),
            current: None,
            show_use: false,
        };
        this.regenerate(cx);
        this
    }

    /// Adds the "Use password" button, for create and edit flows.
    pub fn with_use_button(mut self) -> Self {
        self.show_use = true;
        self
    }

    fn options(&self, cx: &App) -> Options {
        let value = |state: &Entity<SliderState>| state.read(cx).value().start().round() as u8;
        if self.memorable {
            Options::Memorable {
                words: value(&self.words),
                capitalise: self.upper,
                numbers: self.numbers,
                symbols: self.symbols,
            }
        } else {
            Options::Random {
                length: value(&self.length),
                uppercase: self.upper,
                numbers: self.numbers,
                symbols: self.symbols,
            }
        }
    }

    fn regenerate(&mut self, cx: &mut Context<Self>) {
        self.current = generator::generate(&self.options(cx)).ok();
        cx.notify();
    }

    fn set(&mut self, change: impl FnOnce(&mut Self), cx: &mut Context<Self>) {
        change(self);
        self.regenerate(cx);
    }

    /// The password on the LCD, with digits in the LCD's label green and symbols
    /// in its warning tint, so a person can
    /// read it back without mistaking one for the other.
    fn colored(password: &str, cx: &App) -> StyledText {
        let p = cx.palette();
        let highlights = password.char_indices().filter_map(|(start, c)| {
            let token = if c.is_ascii_digit() {
                p.lcd_label
            } else if c.is_ascii_alphabetic() || c == ' ' {
                return None;
            } else {
                p.lcd_warn
            };
            let style = HighlightStyle {
                color: Some(color(token)),
                ..Default::default()
            };
            Some((start..start + c.len_utf8(), style))
        });
        StyledText::new(SharedString::from(password.to_owned())).with_highlights(highlights)
    }

    fn segment(
        &self,
        id: &'static str,
        label: &'static str,
        memorable: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = cx.palette();
        let on = self.memorable == memorable;
        div()
            .id(ElementId::from(id))
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .h(px(34.))
            .rounded(px(8.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(px(14.))
            .cursor_pointer()
            .tab_index(0)
            .focus_visible(|segment| segment.shadow(focus_ring(p, p.well)))
            .map(|segment| {
                if on {
                    segment
                        .bg(color(p.panel))
                        .text_color(color(p.ink))
                        .border_1()
                        .border_color(color(p.edge_key))
                } else {
                    segment.text_color(color(p.ink_2))
                }
            })
            .on_click(
                cx.listener(move |this, _, _, cx| this.set(|this| this.memorable = memorable, cx)),
            )
            .child(label)
    }
}

impl Render for PasswordGenerator {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        let memorable = self.memorable;
        let (slider, unit) = if memorable {
            (&self.words, "Words")
        } else {
            (&self.length, "Characters")
        };
        let amount = slider.read(cx).value().start().round() as u32;
        let (bits, password) = match &self.current {
            Some(generated) => (generated.bits, Some(generated.password.clone())),
            None => (0.0, None),
        };
        let entity = cx.entity();
        let emit = |make: fn(Secret) -> GeneratorEvent, password: &Option<Secret>| {
            let (entity, password) = (entity.clone(), password.clone());
            move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                if let Some(password) = password.clone() {
                    entity.update(cx, |_, cx| cx.emit(make(password)));
                }
            }
        };
        let toggle = |id: &'static str,
                      label: &'static str,
                      checked: bool,
                      change: fn(&mut Self, bool),
                      cx: &mut Context<Self>| {
            let entity = cx.entity();
            Toggle::new(id)
                .label(label)
                .checked(checked)
                .on_change(move |value, _, cx| {
                    let value = *value;
                    entity.update(cx, |this, cx| this.set(|this| change(this, value), cx));
                })
        };

        div()
            .flex()
            .flex_col()
            .gap(px(tokens::SPACE_3))
            .text_token(tokens::BODY)
            .text_color(color(p.ink))
            .child(
                lcd(cx)
                    .flex()
                    .items_center()
                    .gap(px(tokens::SPACE_2))
                    .py(px(14.))
                    .pl(px(tokens::SPACE_5))
                    .pr(px(tokens::SPACE_2))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(tokens::FONT_MONO)
                            .font_weight(FontWeight::MEDIUM)
                            .text_size(px(17.))
                            .line_height(px(24.))
                            .map(|value| match &password {
                                Some(password) => value.child(Self::colored(password.expose(), cx)),
                                None => value
                                    .text_color(color(p.lcd_warn))
                                    .child("COULDN'T GENERATE A PASSWORD"),
                            }),
                    )
                    .child(
                        IconButton::new("generate-again", IconName::Refresh, "Generate again")
                            .on_click(cx.listener(|this, _, _, cx| this.regenerate(cx))),
                    )
                    .child(
                        IconButton::new("copy-generated", IconName::Copy, "Copy password")
                            .on_click(emit(GeneratorEvent::Copy, &password)),
                    ),
            )
            .child(
                StrengthMeter::new(strength::score_for_bits(bits))
                    .detail(format!("{} bits", bits.round())),
            )
            .child(
                div()
                    .flex()
                    .p(px(3.))
                    .rounded(px(tokens::RADIUS_KEY))
                    .bg(color(p.well))
                    .child(self.segment("mode-random", "Random", false, cx))
                    .child(self.segment("mode-memorable", "Memorable", true, cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(tokens::SPACE_2))
                    .child(
                        div().flex().justify_between().child(unit).child(
                            div()
                                .font_family(tokens::FONT_MONO)
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(amount.to_string()),
                        ),
                    )
                    .child(Slider::new(slider).accessibility_label(unit)),
            )
            .child(toggle(
                "generator-upper",
                if memorable {
                    "Capitalise"
                } else {
                    "Uppercase letters"
                },
                self.upper,
                |this, value| this.upper = value,
                cx,
            ))
            .child(toggle(
                "generator-numbers",
                "Numbers",
                self.numbers,
                |this, value| this.numbers = value,
                cx,
            ))
            .child(toggle(
                "generator-symbols",
                if memorable {
                    "Hyphens between words"
                } else {
                    "Symbols"
                },
                self.symbols,
                |this, value| this.symbols = value,
                cx,
            ))
            .when(self.show_use, |generator| {
                generator.child(
                    Button::new("use-password", "Use password")
                        .primary()
                        .size(ButtonSize::Md)
                        .block()
                        .on_click(emit(GeneratorEvent::Use, &password)),
                )
            })
    }
}
