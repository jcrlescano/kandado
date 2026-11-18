//! Every design system component in one window, for comparison with the
//! previews in `docs/design-system`. `cargo run -p kandado --example gallery`
//!
//! It starts in the light theme (Enamel); `--dark` starts in Graphite. The
//! button in the title bar switches.

use gpui_kit::*;
use kandado_host::{assets::Assets, gallery::Gallery, theme};

fn main() {
    let kind = if std::env::args().any(|arg| arg == "--dark") {
        theme::ThemeKind::Graphite
    } else {
        theme::ThemeKind::Enamel
    };
    gpui_kit::application().with_assets(Assets).run(move |cx| {
        gpui_kit::init(cx);
        theme::fonts::register(cx).expect("bundled fonts load");
        theme::apply(kind, None, cx);

        let bounds = Bounds::centered(None, size(px(1200.), px(900.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..component::TitleBar::window_options()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| Gallery::new(window, cx))
        })
        .expect("failed to open the gallery window");
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
    });
}
