//! Renders the whole component gallery headlessly in both themes. This
//! catches a component that panics while laying out or reads a missing asset;
//! how it looks is checked by eye with `cargo run -p kandado --example gallery`.

use gpui_kit::{
    AppContext, Bounds, Point, TestAppContext, WindowBounds, WindowOptions, px, size,
    test::TestWindowExt,
};
use kandado_host::{
    gallery::Gallery,
    theme::{self, KandadoTheme, ThemeKind},
};

#[gpui_kit::test]
fn every_component_renders_in_both_themes(cx: &mut TestAppContext) {
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        theme::fonts::register(cx).expect("bundled fonts load");
        theme::apply(ThemeKind::Vault, None, cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: size(px(1200.), px(900.)),
            })),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| Gallery::new(window, cx))
        })
        .expect("open test window")
        .0
    });

    for kind in [ThemeKind::Vault, ThemeKind::Paper, ThemeKind::Vault] {
        cx.update_window(window, |_, window, cx| {
            theme::apply(kind, Some(window), cx);
            window.render_frame(cx);
            assert_eq!(cx.theme_kind(), kind);
            assert_eq!(cx.palette(), kind.palette());
        })
        .expect("window is open");
    }
}
