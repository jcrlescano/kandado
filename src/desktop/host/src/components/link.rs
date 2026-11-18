use gpui_kit::{
    App, ClickEvent, CursorStyle, ElementId, InteractiveElement as _, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement as _, Styled, Window, div, px,
};

use crate::theme::{KandadoTheme, TextStyles as _, color, tokens};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// A text link that does something in the app ("Forgot password?",
/// "Sign out"). Links that leave the app open the browser.
#[derive(IntoElement)]
pub struct Link {
    id: ElementId,
    label: SharedString,
    on_click: Option<ClickHandler>,
}

impl Link {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            on_click: None,
        }
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for Link {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let link = color(p.link);
        let mut element = div()
            .id(self.id)
            .cursor(CursorStyle::PointingHand)
            .text_token(tokens::BODY_STRONG)
            .text_color(link)
            .rounded(px(tokens::RADIUS_SM))
            .hover(|style| style.underline())
            .child(self.label);
        if let Some(handler) = self.on_click {
            element = element.on_click(move |event, window, cx| handler(event, window, cx));
        }
        element
    }
}
