//! The few places the app talks to Windows directly.

use crate::lock::LockReason;

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("only web addresses can be opened")]
    NotWeb,
    #[error("the system could not open the address: {0}")]
    Failed(String),
}

/// True for addresses that are safe to hand to the system browser.
pub fn is_web_url(url: &str) -> bool {
    url.starts_with("https://")
        || url.starts_with("http://127.0.0.1:")
        || url.starts_with("http://localhost:")
}

/// Opens an address in the default browser. Only `https` (and loopback
/// `http`, for local development) addresses are opened.
pub fn open_url(url: &str) -> Result<(), PlatformError> {
    if !is_web_url(url) {
        return Err(PlatformError::NotWeb);
    }
    open_url_impl(url)
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn open_url_impl(url: &str) -> Result<(), PlatformError> {
    use windows::{
        Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
        core::{HSTRING, w},
    };
    // SAFETY: all arguments are valid wide strings for the call's duration.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(url),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    // Values above 32 mean success.
    if result.0 as usize > 32 {
        Ok(())
    } else {
        Err(PlatformError::Failed(format!("code {}", result.0 as usize)))
    }
}

#[cfg(not(windows))]
fn open_url_impl(_url: &str) -> Result<(), PlatformError> {
    Err(PlatformError::Failed(
        "opening the browser is only built for Windows".into(),
    ))
}

/// Calls `on_event` when the computer goes to sleep or the Windows session is
/// locked, disconnected or switched away. Runs a hidden message window on its
/// own thread for the life of the process.
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn watch_system_events(
    on_event: impl Fn(LockReason) + Send + Sync + 'static,
) -> std::io::Result<()> {
    use std::sync::OnceLock;

    use windows::{
        Win32::{
            Foundation::{HWND, LPARAM, LRESULT, WPARAM},
            System::{
                LibraryLoader::GetModuleHandleW,
                RemoteDesktop::{NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification},
            },
            UI::WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, HWND_MESSAGE, MSG,
                PBT_APMSUSPEND, RegisterClassW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE,
                WM_POWERBROADCAST, WM_WTSSESSION_CHANGE, WNDCLASSW,
            },
        },
        core::w,
    };

    // WTS_CONSOLE_DISCONNECT, WTS_REMOTE_DISCONNECT, WTS_SESSION_LOGOFF, WTS_SESSION_LOCK.
    const SESSION_LEAVING: [u32; 4] = [2, 4, 6, 7];

    static HANDLER: OnceLock<Box<dyn Fn(LockReason) + Send + Sync>> = OnceLock::new();
    HANDLER
        .set(Box::new(on_event))
        .map_err(|_| std::io::Error::other("system events are already being watched"))?;

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if let Some(handler) = HANDLER.get() {
            if message == WM_POWERBROADCAST && wparam.0 as u32 == PBT_APMSUSPEND {
                handler(LockReason::Sleep);
            } else if message == WM_WTSSESSION_CHANGE
                && SESSION_LEAVING.contains(&(wparam.0 as u32))
            {
                handler(LockReason::SessionSwitch);
            }
        }
        // SAFETY: forwards the same arguments the system gave this function.
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    std::thread::Builder::new()
        .name("kandado-system-events".into())
        .spawn(|| {
            // SAFETY: standard message-only window setup, used only on this thread.
            unsafe {
                let Ok(instance) = GetModuleHandleW(None) else {
                    return;
                };
                let class = WNDCLASSW {
                    lpfnWndProc: Some(window_proc),
                    hInstance: instance.into(),
                    lpszClassName: w!("KandadoSystemEvents"),
                    ..Default::default()
                };
                RegisterClassW(&class);
                let Ok(window) = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("KandadoSystemEvents"),
                    w!("Kandado"),
                    WINDOW_STYLE(0),
                    0,
                    0,
                    0,
                    0,
                    Some(HWND_MESSAGE),
                    None,
                    Some(instance.into()),
                    None,
                ) else {
                    return;
                };
                let _ = WTSRegisterSessionNotification(window, NOTIFY_FOR_THIS_SESSION);
                let mut message = MSG::default();
                while GetMessageW(&mut message, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        })
        .map(|_| ())
}

#[cfg(not(windows))]
pub fn watch_system_events(
    _on_event: impl Fn(LockReason) + Send + Sync + 'static,
) -> std::io::Result<()> {
    Ok(())
}

/// The argument a start at sign-in passes, so the app opens in the tray.
pub const BACKGROUND_ARG: &str = "--background";

/// Makes Windows start Kandado when the person signs in, or stops it.
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn set_start_at_login(on: bool) -> Result<(), PlatformError> {
    use windows::{
        Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW},
        core::{HSTRING, w},
    };
    let run = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let name = w!("Kandado");
    let failed = |code: u32| PlatformError::Failed(format!("registry error {code}"));
    if !on {
        // SAFETY: valid constant strings; deleting a missing value is fine.
        let result = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, run, name) };
        // 2 is "not found": already off.
        return match result.0 {
            0 | 2 => Ok(()),
            code => Err(failed(code)),
        };
    }
    let exe = std::env::current_exe().map_err(|error| PlatformError::Failed(error.to_string()))?;
    let command = HSTRING::from(format!("\"{}\" {BACKGROUND_ARG}", exe.display()));
    let bytes = (command.len() + 1) * 2;
    // SAFETY: `command` is a null-terminated wide string of `bytes` bytes
    // that outlives the call.
    let result = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            run,
            name,
            REG_SZ.0,
            Some(command.as_ptr().cast()),
            bytes as u32,
        )
    };
    match result.0 {
        0 => Ok(()),
        code => Err(failed(code)),
    }
}

#[cfg(not(windows))]
pub fn set_start_at_login(_on: bool) -> Result<(), PlatformError> {
    Err(PlatformError::Failed(
        "starting at sign-in is only built for Windows".into(),
    ))
}

/// Shows or hides a window by its native handle. Hiding is how the app stays
/// in the tray; showing also brings the window to the front.
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn set_window_visible(hwnd: isize, visible: bool) {
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{
            IsIconic, SW_HIDE, SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow,
        },
    };
    let window = HWND(hwnd as *mut core::ffi::c_void);
    // SAFETY: the handle comes from the app's own live window; these calls
    // tolerate a stale handle by failing.
    unsafe {
        if visible {
            let command = if IsIconic(window).as_bool() {
                SW_RESTORE
            } else {
                SW_SHOW
            };
            let _ = ShowWindow(window, command);
            let _ = SetForegroundWindow(window);
        } else {
            let _ = ShowWindow(window, SW_HIDE);
        }
    }
}

#[cfg(not(windows))]
pub fn set_window_visible(_hwnd: isize, _visible: bool) {}

/// Turns the global shortcut on and off while the app runs.
#[derive(Debug, Clone, Copy)]
pub struct HotkeyControl {
    #[cfg_attr(not(windows), allow(dead_code))]
    thread: u32,
}

/// WM_APP + 1: turn the hotkey on (wParam 1) or off (0).
#[cfg(windows)]
const SET_HOTKEY: u32 = 0x8001;

/// Listens for Ctrl Shift Space anywhere in Windows and calls `on_press`.
/// Runs on its own thread for the life of the process.
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn watch_global_hotkey(
    enabled: bool,
    on_press: impl Fn() + Send + 'static,
) -> std::io::Result<HotkeyControl> {
    use windows::Win32::{
        System::Threading::GetCurrentThreadId,
        UI::{
            Input::KeyboardAndMouse::{
                MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, UnregisterHotKey, VK_SPACE,
            },
            WindowsAndMessaging::{GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, WM_HOTKEY},
        },
    };

    const HOTKEY_ID: i32 = 1;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("kandado-hotkey".into())
        .spawn(move || {
            let set = |on: bool| {
                // SAFETY: registers or removes a hotkey owned by this thread.
                unsafe {
                    if on {
                        let _ = RegisterHotKey(
                            None,
                            HOTKEY_ID,
                            MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT,
                            u32::from(VK_SPACE.0),
                        );
                    } else {
                        let _ = UnregisterHotKey(None, HOTKEY_ID);
                    }
                }
            };
            // SAFETY: a plain message loop on this thread. PeekMessage first,
            // so the queue exists before the thread id is handed out.
            unsafe {
                let mut message = MSG::default();
                let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
                let _ = sender.send(GetCurrentThreadId());
                set(enabled);
                while GetMessageW(&mut message, None, 0, 0).as_bool() {
                    if message.message == WM_HOTKEY {
                        on_press();
                    } else if message.message == SET_HOTKEY {
                        set(message.wParam.0 != 0);
                    }
                }
            }
        })?;
    let thread = receiver
        .recv()
        .map_err(|_| std::io::Error::other("the shortcut thread stopped"))?;
    Ok(HotkeyControl { thread })
}

impl HotkeyControl {
    #[cfg(windows)]
    #[allow(unsafe_code)]
    pub fn set_enabled(&self, on: bool) {
        use windows::Win32::{
            Foundation::{LPARAM, WPARAM},
            UI::WindowsAndMessaging::PostThreadMessageW,
        };
        // SAFETY: posts a plain message to the app's own hotkey thread.
        let _ = unsafe {
            PostThreadMessageW(self.thread, SET_HOTKEY, WPARAM(usize::from(on)), LPARAM(0))
        };
    }

    #[cfg(not(windows))]
    pub fn set_enabled(&self, _on: bool) {}
}

#[cfg(not(windows))]
pub fn watch_global_hotkey(
    _enabled: bool,
    _on_press: impl Fn() + Send + 'static,
) -> std::io::Result<HotkeyControl> {
    Ok(HotkeyControl { thread: 0 })
}

/// Everything on screen, as brightness values, for finding a QR code (D-20).
pub struct Screenshot {
    pub width: usize,
    pub height: usize,
    /// One byte per pixel, row by row; 0 is black.
    pub luma: Vec<u8>,
}

/// Captures every monitor. The pixels never leave this process and are
/// dropped as soon as the QR search is done.
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn capture_screen() -> Result<Screenshot, PlatformError> {
    use windows::Win32::{
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap,
            CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits,
            ReleaseDC, SRCCOPY, SelectObject,
        },
        UI::WindowsAndMessaging::{
            GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
            SM_YVIRTUALSCREEN,
        },
    };
    let failed = |what: &str| PlatformError::Failed(format!("screen capture: {what}"));
    // SAFETY: standard GDI screen copy. Every object created here is released
    // before returning, and the pixel buffer is sized for the bitmap asked for.
    unsafe {
        let (left, top, width, height) = (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        );
        if width <= 0 || height <= 0 {
            return Err(failed("no screen"));
        }
        let screen = GetDC(None);
        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let previous = SelectObject(memory, bitmap.into());
        let copied = BitBlt(
            memory,
            0,
            0,
            width,
            height,
            Some(screen),
            left,
            top,
            SRCCOPY,
        );

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                // Negative: rows from the top down.
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        // The bitmap must not be selected into a DC while it is read.
        SelectObject(memory, previous);
        let rows = GetDIBits(
            memory,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
        if copied.is_err() || rows == 0 {
            return Err(failed("the screen could not be read"));
        }
        // BGRA to brightness.
        let luma = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                ((u32::from(p[2]) * 299 + u32::from(p[1]) * 587 + u32::from(p[0]) * 114) / 1000)
                    as u8
            })
            .collect();
        Ok(Screenshot {
            width: width as usize,
            height: height as usize,
            luma,
        })
    }
}

#[cfg(not(windows))]
pub fn capture_screen() -> Result<Screenshot, PlatformError> {
    Err(PlatformError::Failed(
        "screen capture is only built for Windows".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_addresses_are_opened() {
        assert!(is_web_url("https://accounts.example.test/start?x=1"));
        assert!(is_web_url("http://127.0.0.1:8080/v1/auth/google/start"));
        for bad in [
            "file:///C:/Windows/System32/calc.exe",
            "calc.exe",
            "javascript:alert(1)",
            "http://evil.test/",
            "ms-settings:privacy",
            "",
        ] {
            assert!(!is_web_url(bad), "{bad}");
            assert!(matches!(open_url(bad), Err(PlatformError::NotWeb)));
        }
    }
}
