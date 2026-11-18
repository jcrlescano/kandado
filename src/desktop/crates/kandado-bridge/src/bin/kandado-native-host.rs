//! The program browsers start for native messaging. It checks which extension
//! started it against the manifests beside it, connects to the desktop app's
//! pipe, and relays frames until either side closes.
//!
//! It holds nothing and decides nothing: the desktop app answers every request.
//! It writes nothing to stdout except frames (the browser reads stdout as the
//! protocol), so errors go to stderr, which browsers log.

use kandado_bridge::{install, relay};

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let Some(origin) = relay::origin_from_args(&args) else {
        eprintln!("kandado-native-host: started without an extension origin");
        return std::process::ExitCode::FAILURE;
    };
    let dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf));
    let allowed = dir
        .as_deref()
        .map(install::read_allowed)
        .unwrap_or_default();
    if !allowed.allows(&origin) {
        eprintln!("kandado-native-host: {origin} is not an allowed extension");
        return std::process::ExitCode::FAILURE;
    }

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("kandado-native-host: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    runtime.block_on(run(origin))
}

#[cfg(windows)]
async fn run(origin: String) -> std::process::ExitCode {
    use kandado_bridge::{pipe, protocol::write_frame};

    // Debug builds can be pointed at a test pipe; release builds use only the real one.
    let name = match std::env::var("KANDADO_BRIDGE_PIPE") {
        Ok(name) if cfg!(debug_assertions) => Ok(name),
        _ => pipe::pipe_name(),
    };
    let connected = name.and_then(|name| pipe::connect(&name));
    let pipe = match connected {
        Ok(pipe) => pipe,
        Err(error) => {
            eprintln!("kandado-native-host: {error}");
            let mut stdout = tokio::io::stdout();
            let _ = write_frame(&mut stdout, relay::UNAVAILABLE).await;
            return std::process::ExitCode::SUCCESS;
        }
    };
    match relay::relay(tokio::io::stdin(), tokio::io::stdout(), pipe, origin).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("kandado-native-host: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(windows))]
async fn run(_origin: String) -> std::process::ExitCode {
    let mut stdout = tokio::io::stdout();
    let _ = kandado_bridge::protocol::write_frame(&mut stdout, relay::UNAVAILABLE).await;
    std::process::ExitCode::SUCCESS
}
