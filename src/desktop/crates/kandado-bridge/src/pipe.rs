//! The named pipe between the native host and the desktop app.
//!
//! Its name carries the Windows user's SID, so two people on one computer get
//! two pipes, and its access list lets only that user in. Remote clients are
//! refused. Windows only; other platforms report `Unsupported` until the
//! desktop app is built for them.

#[derive(Debug, thiserror::Error)]
pub enum PipeError {
    #[error("the desktop link is only built for Windows")]
    Unsupported,
    #[error("another program already owns Kandado's pipe")]
    Taken,
    #[error("the Kandado desktop app is not running")]
    NotRunning,
    #[error("Windows refused: {0}")]
    Os(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[cfg(windows)]
pub use windows_pipe::*;

#[cfg(windows)]
mod windows_pipe {
    use tokio::net::windows::named_pipe::{
        ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
    };

    use super::PipeError;

    /// `\\.\pipe\kandado-bridge-{SID}` for the user this process runs as.
    pub fn pipe_name() -> Result<String, PipeError> {
        Ok(format!(r"\\.\pipe\kandado-bridge-{}", current_user_sid()?))
    }

    /// Creates one instance of the pipe, open to the current user only. The
    /// first instance fails if another program already created the pipe.
    #[allow(unsafe_code)]
    pub fn create_server(name: &str, first: bool) -> Result<NamedPipeServer, PipeError> {
        use windows::{
            Win32::{
                Foundation::{HLOCAL, LocalFree},
                Security::{
                    Authorization::{
                        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
                    },
                    PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
                },
            },
            core::HSTRING,
        };

        // Protected DACL: full access for this user, nobody else.
        let sddl = HSTRING::from(format!("D:P(A;;GA;;;{})", current_user_sid()?));
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: `sddl` is a valid null-terminated wide string; the descriptor
        // is written by Windows and freed below with LocalFree.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                &sddl,
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
        }
        .map_err(|error| PipeError::Os(error.message()))?;
        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };
        let mut options = ServerOptions::new();
        options
            .first_pipe_instance(first)
            .reject_remote_clients(true);
        // SAFETY: `attributes` points at a valid security descriptor that lives
        // until after the call; Windows copies it into the pipe object.
        let created = unsafe {
            options.create_with_security_attributes_raw(
                name,
                (&raw mut attributes).cast::<core::ffi::c_void>(),
            )
        };
        // SAFETY: the descriptor came from LocalAlloc inside the conversion above.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        }
        created.map_err(|error| {
            // ERROR_ACCESS_DENIED on the first instance: someone else made the pipe.
            if first && error.raw_os_error() == Some(5) {
                PipeError::Taken
            } else {
                PipeError::Io(error)
            }
        })
    }

    /// Connects to the desktop app's pipe.
    pub fn connect(name: &str) -> Result<NamedPipeClient, PipeError> {
        ClientOptions::new().open(name).map_err(|error| {
            // ERROR_FILE_NOT_FOUND: nobody serves the pipe.
            if error.raw_os_error() == Some(2) {
                PipeError::NotRunning
            } else {
                PipeError::Io(error)
            }
        })
    }

    /// The string SID (`S-1-5-21-…`) of the user this process runs as.
    #[allow(unsafe_code)]
    pub fn current_user_sid() -> Result<String, PipeError> {
        use windows::{
            Win32::{
                Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree},
                Security::{
                    Authorization::ConvertSidToStringSidW, GetTokenInformation, TOKEN_QUERY,
                    TOKEN_USER, TokenUser,
                },
                System::Threading::{GetCurrentProcess, OpenProcessToken},
            },
            core::PWSTR,
        };

        let os = |error: windows::core::Error| PipeError::Os(error.message());
        let mut token = HANDLE::default();
        // SAFETY: the pseudo-handle of the current process is always valid.
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.map_err(os)?;
        let result = (|| {
            let mut needed = 0u32;
            // SAFETY: a size query with no buffer; it fails with the size needed.
            let _ = unsafe { GetTokenInformation(token, TokenUser, None, 0, &mut needed) };
            let mut buffer = vec![0u8; needed as usize];
            // SAFETY: `buffer` holds `needed` bytes.
            unsafe {
                GetTokenInformation(
                    token,
                    TokenUser,
                    Some(buffer.as_mut_ptr().cast()),
                    needed,
                    &mut needed,
                )
            }
            .map_err(os)?;
            // SAFETY: Windows wrote a TOKEN_USER at the start of the buffer.
            let user = unsafe { &*(buffer.as_ptr().cast::<TOKEN_USER>()) };
            let mut text = PWSTR::null();
            // SAFETY: the SID inside `buffer` is valid while `buffer` lives.
            unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) }.map_err(os)?;
            // SAFETY: `text` is a null-terminated string from LocalAlloc.
            let sid = unsafe { text.to_string() }.map_err(|_| PipeError::Os("bad SID".into()));
            // SAFETY: freeing what ConvertSidToStringSidW allocated.
            unsafe {
                let _ = LocalFree(Some(HLOCAL(text.0.cast())));
            }
            sid
        })();
        // SAFETY: closing the token opened above.
        unsafe {
            let _ = CloseHandle(token);
        }
        result
    }
}

#[cfg(not(windows))]
pub fn pipe_name() -> Result<String, PipeError> {
    Err(PipeError::Unsupported)
}
