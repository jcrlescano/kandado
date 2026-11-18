//! Copying secrets. A copy is marked so Windows keeps it out of clipboard
//! history and the cloud clipboard, and it is cleared after the time chosen
//! at D-44, unless something else was copied in between.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

#[derive(Debug, thiserror::Error)]
#[error("the clipboard could not be used: {0}")]
pub struct ClipboardError(pub String);

/// The system clipboard, or a stand-in for tests.
pub trait ClipboardBackend: Send + Sync {
    /// Puts `text` on the clipboard, marked as a secret, and returns a mark
    /// that identifies this copy.
    fn write_secret(&self, text: &str) -> Result<u64, ClipboardError>;
    /// Empties the clipboard if it still holds the copy with this mark.
    fn clear_if(&self, mark: u64);
}

/// A clipboard that lives in memory, for tests and non-Windows builds.
#[derive(Default)]
pub struct MemoryClipboard {
    state: Mutex<(u64, Option<String>)>,
}

impl MemoryClipboard {
    pub fn text(&self) -> Option<String> {
        self.state.lock().expect("clipboard state").1.clone()
    }

    /// Something else was copied, by another app.
    pub fn overwrite(&self, text: &str) {
        let mut state = self.state.lock().expect("clipboard state");
        state.0 += 1;
        state.1 = Some(text.to_owned());
    }
}

impl ClipboardBackend for MemoryClipboard {
    fn write_secret(&self, text: &str) -> Result<u64, ClipboardError> {
        let mut state = self.state.lock().expect("clipboard state");
        state.0 += 1;
        state.1 = Some(text.to_owned());
        Ok(state.0)
    }

    fn clear_if(&self, mark: u64) {
        let mut state = self.state.lock().expect("clipboard state");
        if state.0 == mark {
            state.1 = None;
        }
    }
}

/// Copies with a timed clear. Only the latest copy's timer clears anything.
pub struct Clipboard {
    backend: Arc<dyn ClipboardBackend>,
    /// The mark of the latest copy, or 0 when nothing of ours is there.
    latest: Arc<AtomicU64>,
}

impl Clipboard {
    pub fn new(backend: Arc<dyn ClipboardBackend>) -> Self {
        Self {
            backend,
            latest: Arc::new(AtomicU64::new(0)),
        }
    }

    /// The clipboard of this computer.
    pub fn system() -> Self {
        #[cfg(windows)]
        let backend: Arc<dyn ClipboardBackend> = Arc::new(windows_clipboard::WindowsClipboard);
        #[cfg(not(windows))]
        let backend: Arc<dyn ClipboardBackend> = Arc::new(MemoryClipboard::default());
        Self::new(backend)
    }

    /// Copies `text` and clears it after `clear_after`, if given.
    pub fn copy(&self, text: &str, clear_after: Option<Duration>) -> Result<(), ClipboardError> {
        let mark = self.backend.write_secret(text)?;
        self.latest.store(mark, Ordering::SeqCst);
        if let Some(delay) = clear_after {
            let (backend, latest) = (self.backend.clone(), self.latest.clone());
            std::thread::Builder::new()
                .name("kandado-clipboard".into())
                .spawn(move || {
                    std::thread::sleep(delay);
                    // A newer copy has its own timer.
                    if latest
                        .compare_exchange(mark, 0, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        backend.clear_if(mark);
                    }
                })
                .map_err(|error| ClipboardError(error.to_string()))?;
        }
        Ok(())
    }

    /// Clears what Kandado last copied, if it is still there. Called on lock
    /// and on quit.
    pub fn clear_now(&self) {
        let mark = self.latest.swap(0, Ordering::SeqCst);
        if mark != 0 {
            self.backend.clear_if(mark);
        }
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows_clipboard {
    use windows::{
        Win32::{
            Foundation::{GlobalFree, HANDLE, HGLOBAL},
            System::{
                DataExchange::{
                    CloseClipboard, EmptyClipboard, GetClipboardSequenceNumber, OpenClipboard,
                    RegisterClipboardFormatW, SetClipboardData,
                },
                Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
            },
        },
        core::{PCWSTR, w},
    };

    use super::{ClipboardBackend, ClipboardError};

    const CF_UNICODETEXT: u32 = 13;

    pub struct WindowsClipboard;

    /// Closes the clipboard when dropped.
    struct Open;

    impl Open {
        /// Another program may hold the clipboard for a moment; try a few times.
        fn new() -> Result<Self, ClipboardError> {
            for _ in 0..10 {
                // SAFETY: no window owns the clipboard; closed again in `drop`.
                if unsafe { OpenClipboard(None) }.is_ok() {
                    return Ok(Self);
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(ClipboardError("another program is using it".into()))
        }
    }

    impl Drop for Open {
        fn drop(&mut self) {
            // SAFETY: this thread opened the clipboard in `new`.
            let _ = unsafe { CloseClipboard() };
        }
    }

    /// Hands `bytes` to the clipboard under `format`. The clipboard must be open.
    fn set(format: u32, bytes: &[u8]) -> Result<(), ClipboardError> {
        let failed = |error: windows::core::Error| ClipboardError(error.to_string());
        // SAFETY: the block is allocated for `bytes.len()` bytes, written
        // while locked, and owned by the clipboard once SetClipboardData
        // succeeds; on failure it is freed here.
        unsafe {
            let memory: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).map_err(failed)?;
            let target = GlobalLock(memory).cast::<u8>();
            if target.is_null() {
                let _ = GlobalFree(Some(memory));
                return Err(ClipboardError("out of memory".into()));
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
            let _ = GlobalUnlock(memory);
            if let Err(error) = SetClipboardData(format, Some(HANDLE(memory.0))) {
                let _ = GlobalFree(Some(memory));
                return Err(failed(error));
            }
        }
        Ok(())
    }

    fn format(name: PCWSTR) -> u32 {
        // SAFETY: `name` is a valid, null-terminated wide string literal.
        unsafe { RegisterClipboardFormatW(name) }
    }

    impl ClipboardBackend for WindowsClipboard {
        fn write_secret(&self, text: &str) -> Result<u64, ClipboardError> {
            let mut wide: Vec<u8> = text
                .encode_utf16()
                .chain(std::iter::once(0))
                .flat_map(u16::to_le_bytes)
                .collect();
            let result = (|| {
                let _open = Open::new()?;
                // SAFETY: the clipboard is open on this thread.
                unsafe { EmptyClipboard() }.map_err(|error| ClipboardError(error.to_string()))?;
                set(CF_UNICODETEXT, &wide)?;
                // Keep the secret out of clipboard history (Win+V), the cloud
                // clipboard and clipboard managers that honour these formats.
                let no = 0u32.to_le_bytes();
                set(
                    format(w!("ExcludeClipboardContentFromMonitorProcessing")),
                    &no,
                )?;
                set(format(w!("CanIncludeInClipboardHistory")), &no)?;
                set(format(w!("CanUploadToCloudClipboard")), &no)?;
                Ok(())
            })();
            zeroize::Zeroize::zeroize(&mut wide);
            result?;
            // SAFETY: no arguments; reads a counter.
            Ok(u64::from(unsafe { GetClipboardSequenceNumber() }))
        }

        fn clear_if(&self, mark: u64) {
            // SAFETY: no arguments; reads a counter.
            if u64::from(unsafe { GetClipboardSequenceNumber() }) != mark {
                // Something else was copied since. Leave it alone.
                return;
            }
            if let Ok(_open) = Open::new() {
                // SAFETY: the clipboard is open on this thread.
                let _ = unsafe { EmptyClipboard() };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clipboard() -> (Clipboard, Arc<MemoryClipboard>) {
        let backend = Arc::new(MemoryClipboard::default());
        (Clipboard::new(backend.clone()), backend)
    }

    fn wait() {
        std::thread::sleep(Duration::from_millis(250));
    }

    #[test]
    fn a_copy_is_cleared_after_its_time() {
        let (clipboard, backend) = clipboard();
        clipboard
            .copy("hunter2", Some(Duration::from_millis(40)))
            .unwrap();
        assert_eq!(backend.text().as_deref(), Some("hunter2"));
        wait();
        assert_eq!(backend.text(), None);
    }

    #[test]
    fn never_means_it_stays() {
        let (clipboard, backend) = clipboard();
        clipboard.copy("hunter2", None).unwrap();
        wait();
        assert_eq!(backend.text().as_deref(), Some("hunter2"));
    }

    #[test]
    fn what_someone_copied_afterwards_is_left_alone() {
        let (clipboard, backend) = clipboard();
        clipboard
            .copy("hunter2", Some(Duration::from_millis(40)))
            .unwrap();
        backend.overwrite("a shopping list");
        wait();
        assert_eq!(backend.text().as_deref(), Some("a shopping list"));
    }

    #[test]
    fn a_newer_copy_is_not_cleared_by_an_older_timer() {
        let (clipboard, backend) = clipboard();
        clipboard
            .copy("first", Some(Duration::from_millis(40)))
            .unwrap();
        clipboard
            .copy("second", Some(Duration::from_secs(60)))
            .unwrap();
        wait();
        assert_eq!(backend.text().as_deref(), Some("second"));
        // Locking clears it at once.
        clipboard.clear_now();
        assert_eq!(backend.text(), None);
    }

    /// The real Windows clipboard: write, check the mark, clear. Ignored by
    /// default because it replaces whatever the developer has copied; run it
    /// with `cargo test -p kandado-app -- --ignored windows_clipboard`.
    #[ignore = "replaces the clipboard of whoever runs it"]
    #[cfg(windows)]
    #[test]
    fn the_windows_clipboard_takes_and_clears_a_secret() {
        use super::windows_clipboard::WindowsClipboard;
        let backend = WindowsClipboard;
        let Ok(mark) = backend.write_secret("kandado-clipboard-test") else {
            // No interactive desktop (a CI service session): nothing to test.
            return;
        };
        // A stale mark clears nothing; the right one does not panic.
        backend.clear_if(mark.wrapping_sub(1));
        backend.clear_if(mark);
    }
}
