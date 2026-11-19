use gpui_kit::{
    App, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div,
    prelude::FluentBuilder as _, px,
};

use super::{Badge, Tone, hardware::well};
use crate::theme::{KandadoTheme, TextStyles as _, color, tokens};

/// A four-segment strength bar sunk into a well, with its word.
///
/// The score comes from `kandado_model::strength`; this only draws it. The
/// word always shows, so strength never rests on colour alone.
#[derive(IntoElement)]
pub struct StrengthMeter {
    score: u8,
    detail: Option<SharedString>,
}

impl StrengthMeter {
    /// `score` is 0 to 4: Weak, Weak, Fair, Good, Strong.
    pub fn new(score: u8) -> Self {
        Self {
            score: score.min(4),
            detail: None,
        }
    }

    /// The estimate ("98 bits"), or on Monitor the reason ("Used on 3 sites").
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    fn word_and_tone(score: u8) -> (&'static str, Tone) {
        match score {
            0 | 1 => ("Weak", Tone::Danger),
            2 => ("Fair", Tone::Warning),
            3 => ("Good", Tone::Teal),
            _ => ("Strong", Tone::Teal),
        }
    }
}

impl RenderOnce for StrengthMeter {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let (word, tone) = Self::word_and_tone(self.score);
        // A score of 0 still lights one segment: an empty bar reads as "no meter".
        let lit = self.score.max(1);
        div()
            .flex()
            .items_center()
            .gap(px(tokens::SPACE_3))
            .child(
                well(cx)
                    .flex()
                    .flex_1()
                    .max_w(px(200.))
                    .gap(px(tokens::SPACE_1))
                    .p(px(3.))
                    .rounded(px(tokens::RADIUS_ROUND))
                    .children((1..=4u8).map(|segment| {
                        div()
                            .flex_1()
                            .h(px(6.))
                            .rounded(px(tokens::RADIUS_ROUND))
                            .bg(if segment <= lit {
                                tone.solid(p)
                            } else {
                                color(p.edge)
                            })
                    })),
            )
            .child(Badge::new(word).tone(tone))
            .when_some(self.detail, |meter, detail| {
                meter.child(
                    div()
                        .text_token(tokens::SMALL)
                        .text_color(color(p.ink_2))
                        .child(detail),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_map_to_the_design_systems_words_and_tones() {
        let words: Vec<_> = (0..=4).map(StrengthMeter::word_and_tone).collect();
        assert_eq!(
            words,
            [
                ("Weak", Tone::Danger),
                ("Weak", Tone::Danger),
                ("Fair", Tone::Warning),
                ("Good", Tone::Teal),
                ("Strong", Tone::Teal),
            ]
        );
        assert_eq!(StrengthMeter::new(9).score, 4);
    }
}
