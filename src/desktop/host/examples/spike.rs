//! Opens the phase 0 spike window, with a tray icon (D-53).
//! `cargo run -p kandado --example spike`

use std::time::Duration;

use gpui_kit::{
    component::{WindowExt, notification::Notification},
    *,
};
use kandado_host::spike::{Spike, window_options};
use tray_icon::{
    Icon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};

/// A plain teal square; the real mark arrives with the design tokens.
fn tray_image() -> Icon {
    const SIZE: u32 = 32;
    let pixels: Vec<u8> = (0..SIZE * SIZE)
        .flat_map(|_| [0x1F, 0x8A, 0x70, 0xFF])
        .collect();
    Icon::from_rgba(pixels, SIZE, SIZE).expect("valid icon size")
}

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            kandado_host::theme::apply(Default::default(), None, cx);
            let options = window_options(cx);
            let (window, _) = gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| Spike::new(window, cx))
            })
            .expect("failed to open the spike window");

            // The tray icon lives on the main thread, whose message loop GPUI runs.
            let open = MenuItem::new("Open Kandado", true, None);
            let lock = MenuItem::new("Lock", true, None);
            let generate = MenuItem::new("Generate password", true, None);
            let quit = MenuItem::new("Quit", true, None);
            let menu = Menu::new();
            menu.append_items(&[&open, &lock, &generate, &quit])
                .expect("tray menu");
            let tray = TrayIconBuilder::new()
                .with_tooltip("Kandado (spike)")
                .with_icon(tray_image())
                .with_menu(Box::new(menu))
                .build()
                .expect("tray icon");

            let (open, lock, generate, quit) = (
                open.id().clone(),
                lock.id().clone(),
                generate.id().clone(),
                quit.id().clone(),
            );
            cx.spawn(async move |cx| {
                // Keeps the icon alive for as long as the app runs.
                let _tray = tray;
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                    while let Ok(event) = MenuEvent::receiver().try_recv() {
                        let toast = |message: &'static str, cx: &mut AsyncApp| {
                            let _ = window.update(cx, |_, window, cx| {
                                window.activate_window();
                                window.push_notification(Notification::new().message(message), cx);
                            });
                        };
                        if event.id == quit {
                            cx.update(|cx| cx.quit());
                        } else if event.id == open {
                            let _ = window.update(cx, |_, window, _| window.activate_window());
                        } else if event.id == lock {
                            toast("Locked from the tray", cx);
                        } else if event.id == generate {
                            toast("Password generated and copied", cx);
                        }
                    }
                }
            })
            .detach();

            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
        });
}
