//! Console-binary support helpers.
//!
//! These keep the double-clickable `.exe` console tools friendly: when a binary
//! hits a fatal error we print a readable, source-chain-aware report and (when
//! running interactively) hold the window open so the operator can actually read
//! it before it closes. On Windows a console process spawned by double-click
//! tears down its window the instant `main` returns, which otherwise hides the
//! error entirely.

use std::error::Error;
use std::io::{IsTerminal, Read, Write};

/// Print a fatal error to stderr as a readable block, including the full
/// `source()` chain so nested causes (config paths, I/O details) are visible.
pub fn report_fatal_error(error: &dyn Error) {
    let mut stderr = std::io::stderr();
    let _ = writeln!(stderr, "\nERROR: {error}");

    let mut source = error.source();
    while let Some(cause) = source {
        let _ = writeln!(stderr, "  caused by: {cause}");
        source = cause.source();
    }

    let _ = stderr.flush();
}

/// Hold the console window open until the operator presses Enter.
///
/// Skipped automatically when stdin is not a terminal (pipes, redirects, CI) or
/// when `SOS_NO_PAUSE` is set, so automated runs never hang.
pub fn pause_before_exit() {
    if std::env::var_os("SOS_NO_PAUSE").is_some() {
        return;
    }

    if !std::io::stdin().is_terminal() {
        return;
    }

    let mut stderr = std::io::stderr();
    let _ = write!(stderr, "\nPress Enter to close this window...");
    let _ = stderr.flush();

    let _ = std::io::stdin().read(&mut [0u8]);
}

/// Build an actionable message for a failed connection to the RL Stats API,
/// including the underlying error and the usual things to check.
pub fn explain_connection_failure(
    host: &str,
    port: u16,
    err: &dyn Error,
) -> String {
    format!(
        "Could not connect to the RL Stats API at {host}:{port} after multiple retries (~5s).\n\
         Underlying error: {err}\n\
         \n\
         Common causes:\n\
         - Rocket League is not running. Start the game first, then relaunch this tool.\n\
         - The in-game Stats API exporter is not enabled. Check DefaultStatsAPI.ini:\n\
         \x20    [TAGame.MatchStatsExporter_TA]\n\
         \x20    Port={port}\n\
         \x20    PacketSendRate=60   ; must be greater than 0\n\
         - Another program is already using port {port}, or a custom --rl-port was passed.\n\
         - A firewall is blocking local TCP connections on 127.0.0.1."
    )
}
