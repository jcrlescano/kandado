//! Drives the phase 0 spike headlessly: real components, real hit testing,
//! simulated input. These are the repeatable half of the GPUI go / no-go; the
//! other half is looking at `cargo run -p kandado --example spike`.

use gpui_kit::{
    App, AppContext, Bounds, ElementInputHandler, Entity, InputHandler, Pixels, Point, ScrollDelta,
    Size, TestAppContext, Window, WindowBounds, WindowHandle, WindowOptions,
    base::Root,
    component::{ActiveTheme, WindowExt},
    point, px, size,
    test::TestWindowExt,
};
use kandado_host::spike::{RAIL_BREAKPOINT, RAIL_WIDTH, ROW_COUNT, SIDEBAR_WIDTH, Spike};

fn open(cx: &mut TestAppContext, window_size: Size<Pixels>) -> (WindowHandle<Root>, Entity<Spike>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        kandado_host::theme::apply(Default::default(), None, cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: window_size,
            })),
            ..Default::default()
        };
        let (window, spike) = gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| Spike::new(window, cx))
        })
        .expect("open test window");
        (window.downcast::<Root>().expect("Base Root"), spike)
    })
}

fn in_window<R>(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    run: impl FnOnce(&mut Window, &mut App) -> R,
) -> R {
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        run(window, cx)
    })
    .expect("window is open")
}

fn sidebar_width(cx: &mut TestAppContext, handle: WindowHandle<Root>) -> Pixels {
    in_window(cx, handle, |window, _| {
        window.find("sidebar").bounds().size.width
    })
}

#[gpui_kit::test]
fn the_sidebar_collapses_to_a_rail_below_1120_pixels(cx: &mut TestAppContext) {
    let (handle, _) = open(cx, size(px(1280.), px(800.)));
    assert_eq!(sidebar_width(cx, handle), SIDEBAR_WIDTH);

    cx.simulate_window_resize(handle.into(), size(RAIL_BREAKPOINT - px(1.), px(800.)));
    assert_eq!(sidebar_width(cx, handle), RAIL_WIDTH);

    cx.simulate_window_resize(handle.into(), size(RAIL_BREAKPOINT, px(800.)));
    assert_eq!(sidebar_width(cx, handle), SIDEBAR_WIDTH);
}

#[gpui_kit::test]
fn the_three_panes_fill_the_window_side_by_side(cx: &mut TestAppContext) {
    let (handle, _) = open(cx, size(px(1280.), px(800.)));
    in_window(cx, handle, |window, _| {
        let sidebar = window.find("sidebar").bounds();
        let list = window.find("item-list").bounds();
        let detail = window.find("detail").bounds();
        assert_eq!(sidebar.origin.x, px(0.));
        assert_eq!(list.origin.x, sidebar.right());
        assert_eq!(detail.origin.x, list.right());
        assert_eq!(detail.right(), px(1280.));
        // The search field sits in the middle of the title bar.
        let search = window.find("search").bounds();
        assert!((search.center().x - px(640.)).abs() < px(2.), "{search:?}");
        assert!(search.bottom() <= sidebar.top());
    });
}

#[gpui_kit::test]
fn a_5000_row_list_only_builds_the_rows_on_screen(cx: &mut TestAppContext) {
    let (handle, _) = open(cx, size(px(1280.), px(800.)));
    in_window(cx, handle, |window, cx| {
        let built = |window: &Window| {
            (0..ROW_COUNT)
                .filter(|&index| window.try_find(("row", index)).is_some())
                .count()
        };
        assert!(window.try_find(("row", 0usize)).is_some());
        assert!(window.try_find(("row", ROW_COUNT - 1)).is_none());
        let at_top = built(window);
        assert!((10..40).contains(&at_top), "{at_top} rows built at the top");

        // Scroll far past the end; the list clamps to its last row.
        window.scroll(
            "item-list",
            ScrollDelta::Pixels(point(px(0.), px(-1_000_000.))),
            cx,
        );
        window.render_frame(cx);
        assert!(window.try_find(("row", 0usize)).is_none());
        assert!(window.try_find(("row", ROW_COUNT - 1)).is_some());
        assert!((10..40).contains(&built(window)));

        // Rows stay interactive after scrolling.
        window.click(("row", ROW_COUNT - 1), cx);
    });
}

#[gpui_kit::test]
fn text_input_takes_typing_and_ime_composition(cx: &mut TestAppContext) {
    let (handle, spike) = open(cx, size(px(1280.), px(800.)));
    let title = cx.update(|cx| spike.read(cx).title().clone());
    in_window(cx, handle, |window, cx| {
        window.click("title", cx);
        window.press("ctrl-a", cx);
        window.input("Banco 中文 🦀", cx);
        assert_eq!(window.find("title").value(), Some("Banco 中文 🦀"));

        // An input method composing Japanese: marked text first, then the commit.
        window.press("ctrl-a", cx);
        let bounds = window.find("title").bounds();
        let mut ime = ElementInputHandler::new(bounds, title.clone());
        ime.replace_and_mark_text_in_range(None, "にほん", Some(3..3), window, cx);
        window.render_frame(cx);
        assert_eq!(window.find("title").value(), Some("にほん"));
        assert_eq!(ime.marked_text_range(window, cx), Some(0..3));

        ime.replace_and_mark_text_in_range(None, "日本", Some(2..2), window, cx);
        ime.replace_text_in_range(None, "日本", window, cx);
        window.render_frame(cx);
        assert_eq!(window.find("title").value(), Some("日本"));
        assert_eq!(ime.marked_text_range(window, cx), None);
    });
    cx.update(|cx| assert_eq!(title.read(cx).value(), "日本"));
}

#[gpui_kit::test]
fn a_masked_field_keeps_its_value_out_of_the_accessibility_tree(cx: &mut TestAppContext) {
    let (handle, spike) = open(cx, size(px(1280.), px(800.)));
    let password = cx.update(|cx| spike.read(cx).password().clone());
    in_window(cx, handle, |window, cx| {
        let shown = window.find("password").value().map(str::to_owned);
        assert_ne!(shown.as_deref(), Some("correct horse battery"));
        assert!(!shown.unwrap_or_default().contains("horse"));

        window.click("password", cx);
        window.press("ctrl-a", cx);
        window.input("nuevo secreto", cx);
        let shown = window.find("password").value().map(str::to_owned);
        assert!(!shown.unwrap_or_default().contains("secreto"));
    });
    // The owner of the field still reads the real value.
    cx.update(|cx| assert_eq!(password.read(cx).value(), "nuevo secreto"));
}

#[gpui_kit::test]
fn dialog_popover_and_toast_open(cx: &mut TestAppContext) {
    let (handle, _) = open(cx, size(px(1280.), px(800.)));
    in_window(cx, handle, |window, cx| {
        assert!(window.try_find("popover-body").is_none());
        window.click("open-popover", cx);
        window.render_frame(cx);
        assert!(window.try_find("popover-body").is_some());
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(window.try_find("popover-body").is_none());

        assert!(window.notifications(cx).is_empty());
        window.click("show-toast", cx);
        assert_eq!(window.notifications(cx).len(), 1);

        assert!(!window.has_active_dialog(cx));
        window.click("open-dialog", cx);
        assert!(window.has_active_dialog(cx));
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx));
    });
}

#[gpui_kit::test]
fn the_theme_switches_between_light_and_dark(cx: &mut TestAppContext) {
    let (handle, _) = open(cx, size(px(1280.), px(800.)));
    in_window(cx, handle, |window, cx| {
        let started_dark = cx.theme().is_dark();
        let background = cx.theme().background;
        window.click("toggle-theme", cx);
        window.render_frame(cx);
        assert_eq!(cx.theme().is_dark(), !started_dark);
        assert_ne!(cx.theme().background, background);
        window.click("toggle-theme", cx);
        window.render_frame(cx);
        assert_eq!(cx.theme().is_dark(), started_dark);
    });
}
