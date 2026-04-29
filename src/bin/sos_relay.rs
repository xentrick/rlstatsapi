use std::path::PathBuf;
use std::time::Duration;

use futures_util::SinkExt;
use rlstatsapi::{
    ClientOptions, RocketLeagueStatsClient, SosEnvelope, translate_stats_event,
};
use tokio::net::TcpStream;
#[cfg(unix)]
use tokio::signal::unix::{SignalKind, signal as unix_signal};
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use url::Url;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug, Clone)]
struct CliOptions {
    ini_path: Option<PathBuf>,
    rl_host: Option<String>,
    rl_port: Option<u16>,
    ws_target: WsTarget,
    debug: bool,
    reconnect_ms: u64,
    max_events: Option<usize>,
}

impl CliOptions {
    fn ws_url(&self) -> String {
        self.ws_target.to_url()
    }
}

#[derive(Debug, Clone)]
struct WsTarget {
    scheme: String,
    host: String,
    port: u16,
    path: String,
}

impl WsTarget {
    fn to_url(&self) -> String {
        format!("{}://{}:{}{}", self.scheme, self.host, self.port, self.path)
    }
}

fn debug_log(cli: &CliOptions, message: impl AsRef<str>) {
    if cli.debug {
        eprintln!("[debug] {}", message.as_ref());
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = parse_args()?;
    debug_log(
        &cli,
        format!(
            "startup: rl_host={:?} rl_port={:?} ws_url={} reconnect_ms={} max_events={:?}",
            cli.rl_host,
            cli.rl_port,
            cli.ws_url(),
            cli.reconnect_ms,
            cli.max_events
        ),
    );

    let mut options = ClientOptions::default();
    if let Some(path) = cli.ini_path.clone() {
        options.stats_api_ini_path = Some(path);
    } else {
        options.auto_enable_packet_rate = false;
    }

    if let Some(host) = cli.rl_host.clone() {
        options.host = host;
    }
    if let Some(port) = cli.rl_port {
        options.port_override = Some(port);
    }

    let mut client = RocketLeagueStatsClient::connect_with_retry(
        options,
        20,
        Duration::from_millis(250),
    )
    .await?;
    debug_log(
        &cli,
        format!(
            "connected to RL stats source {}",
            client.connection().socket_address()
        ),
    );

    println!(
        "Relay source={} destination={}",
        client.connection().socket_address(),
        cli.ws_url()
    );

    let mut ws_stream = connect_ws_with_retry(&cli).await?;

    let mut relayed = 0usize;
    let shutdown_signal = wait_for_shutdown_signal();
    tokio::pin!(shutdown_signal);

    loop {
        tokio::select! {
            signal_name = &mut shutdown_signal => {
                eprintln!("Received {signal_name}; shutting down relay...");
                graceful_overlay_shutdown(&mut ws_stream).await;
                return Ok(());
            }
            next_event = client.next_event() => {
                match next_event {
                    Ok(Some(event)) => {
                        for envelope in translate_stats_event(&event)? {
                            send_envelope_with_retry(&cli, &mut ws_stream, &envelope).await?;
                            relayed += 1;

                            debug_log(
                                &cli,
                                format!(
                                    "relayed event '{}' (total sent: {})",
                                    envelope.event, relayed
                                ),
                            );

                            if cli.max_events.is_some_and(|max| relayed >= max) {
                                debug_log(
                                    &cli,
                                    format!("max-events reached ({}), exiting", relayed),
                                );
                                graceful_overlay_shutdown(&mut ws_stream).await;
                                return Ok(());
                            }
                        }
                    }
                    Ok(None) => {
                        eprintln!("RL event stream closed; attempting reconnect...");
                        reconnect_rl_with_backoff(&mut client, &cli).await?;
                    }
                    Err(error) => {
                        eprintln!("RL stream error: {error}; attempting reconnect...");
                        reconnect_rl_with_backoff(&mut client, &cli).await?;
                    }
                }
            }
        }
    }
}

async fn wait_for_shutdown_signal() -> &'static str {
    #[cfg(unix)]
    {
        match unix_signal(SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => "SIGINT",
                    _ = terminate.recv() => "SIGTERM",
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
                "SIGINT"
            }
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
        "SIGINT"
    }
}

async fn graceful_overlay_shutdown(ws_stream: &mut WsStream) {
    if let Err(error) = ws_stream.close(None).await {
        eprintln!("overlay websocket close failed: {error}");
    } else {
        eprintln!("overlay websocket closed");
    }
}

async fn reconnect_rl_with_backoff(
    client: &mut RocketLeagueStatsClient,
    cli: &CliOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut last_error: Option<Box<dyn std::error::Error>> = None;

    for attempt in 1..=10 {
        debug_log(
            &cli,
            format!("attempting RL reconnect (attempt {attempt}/10)"),
        );
        match client.reconnect().await {
            Ok(()) => {
                debug_log(
                    &cli,
                    format!(
                        "RL reconnect succeeded; source={} ",
                        client.connection().socket_address()
                    ),
                );
                return Ok(());
            }
            Err(error) => {
                debug_log(
                    &cli,
                    format!(
                        "RL reconnect failed on attempt {attempt}/10: {error}"
                    ),
                );
                last_error = Some(Box::new(error));
                sleep(Duration::from_millis(cli.reconnect_ms)).await;
            }
        }
    }

    match last_error {
        Some(error) => Err(error),
        None => Err("reconnect failed".into()),
    }
}

async fn connect_ws_with_retry(
    cli: &CliOptions,
) -> Result<WsStream, Box<dyn std::error::Error>> {
    let mut attempt = 1usize;

    loop {
        let destination = cli.ws_url();
        debug_log(
            cli,
            format!(
                "attempting overlay websocket connection (attempt {}): {}",
                attempt, destination
            ),
        );

        match connect_async(&destination).await {
            Ok((stream, response)) => {
                debug_log(
                    cli,
                    format!(
                        "overlay websocket connected (status {}): {}",
                        response.status(),
                        destination
                    ),
                );
                return Ok(stream);
            }
            Err(error) => {
                eprintln!(
                    "WebSocket connection failed ({}): {}; retrying...",
                    destination, error
                );
                sleep(Duration::from_millis(cli.reconnect_ms)).await;
                attempt += 1;
            }
        }
    }
}

async fn send_envelope_with_retry(
    cli: &CliOptions,
    ws_stream: &mut WsStream,
    envelope: &SosEnvelope,
) -> Result<(), Box<dyn std::error::Error>> {
    let payload = serde_json::to_string(envelope)?;
    let mut attempt = 1usize;

    loop {
        debug_log(
            cli,
            format!(
                "sending envelope '{}' (attempt {}, {} bytes)",
                envelope.event,
                attempt,
                payload.len()
            ),
        );

        match ws_stream.send(Message::Text(payload.clone().into())).await {
            Ok(()) => {
                debug_log(cli, format!("envelope '{}' sent", envelope.event));
                return Ok(());
            }
            Err(error) => {
                eprintln!("WebSocket send failed: {error}; reconnecting...");
                *ws_stream = connect_ws_with_retry(cli).await?;
                attempt += 1;
            }
        }
    }
}

fn parse_args() -> Result<CliOptions, Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);

    let mut ini_path = None;
    let mut rl_host = None;
    let mut rl_port = None;
    let mut ws_host = "127.0.0.1".to_string();
    let mut ws_port: Option<u16> = None;
    let mut ws_path: Option<String> = None;
    let mut debug = false;
    let mut reconnect_ms = 500u64;
    let mut max_events = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--ini" => {
                let value = args.next().ok_or("--ini requires a file path")?;
                ini_path = Some(PathBuf::from(value));
            }
            "--host" | "--rl-host" => {
                let value = args.next().ok_or("--rl-host requires a value")?;
                rl_host = Some(value);
            }
            "--port" | "--rl-port" => {
                let value = args.next().ok_or("--rl-port requires a value")?;
                rl_port = Some(value.parse::<u16>()?);
            }
            "--ws-host" => {
                let value = args.next().ok_or("--ws-host requires a value")?;
                ws_host = value;
            }
            "--ws-port" => {
                let value = args.next().ok_or("--ws-port requires a value")?;
                ws_port = Some(value.parse::<u16>()?);
            }
            "--ws-path" => {
                let value = args.next().ok_or("--ws-path requires a value")?;
                ws_path = Some(normalize_ws_path(&value));
            }
            "--reconnect-ms" => {
                let value =
                    args.next().ok_or("--reconnect-ms requires a value")?;
                reconnect_ms = value.parse::<u64>()?;
            }
            "-d" | "--debug" => {
                debug = true;
            }
            "--max-events" => {
                let value =
                    args.next().ok_or("--max-events requires a value")?;
                max_events = Some(value.parse::<usize>()?);
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => {
                return Err(format!("Unknown argument: {other}").into());
            }
        }
    }

    let ws_target = build_ws_target(&ws_host, ws_port, ws_path.as_deref())?;

    Ok(CliOptions {
        ini_path,
        rl_host,
        rl_port,
        ws_target,
        debug,
        reconnect_ms,
        max_events,
    })
}

fn build_ws_target(
    ws_host_input: &str,
    ws_port_override: Option<u16>,
    ws_path_override: Option<&str>,
) -> Result<WsTarget, Box<dyn std::error::Error>> {
    let trimmed = ws_host_input.trim();
    if trimmed.is_empty() {
        return Err("--ws-host cannot be empty".into());
    }

    let has_explicit_scheme = trimmed.contains("://");
    let parsed_url = if has_explicit_scheme {
        Url::parse(trimmed)?
    } else {
        Url::parse(&format!("ws://{trimmed}"))?
    };

    let scheme = map_ws_scheme(parsed_url.scheme())?;
    let host = parsed_url
        .host_str()
        .ok_or("--ws-host must include a hostname")?
        .to_string();

    let port = match (ws_port_override, parsed_url.port(), has_explicit_scheme)
    {
        (Some(port), _, _) => port,
        (None, Some(port), _) => port,
        (None, None, true) => default_port_for_ws_scheme(scheme),
        (None, None, false) => 49122,
    };

    let path = match ws_path_override {
        Some(path) => normalize_ws_path(path),
        None => normalize_ws_path(parsed_url.path()),
    };

    Ok(WsTarget {
        scheme: scheme.to_string(),
        host,
        port,
        path,
    })
}

fn map_ws_scheme(
    scheme: &str,
) -> Result<&'static str, Box<dyn std::error::Error>> {
    match scheme {
        "ws" | "http" => Ok("ws"),
        "wss" | "https" => Ok("wss"),
        other => Err(
            format!("unsupported --ws-host scheme '{other}' (use ws, wss, http, or https)")
                .into(),
        ),
    }
}

fn default_port_for_ws_scheme(scheme: &str) -> u16 {
    match scheme {
        "wss" => 443,
        _ => 80,
    }
}

fn normalize_ws_path(path: &str) -> String {
    if path.is_empty() || path == "/" {
        "/".to_string()
    } else if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

fn print_help() {
    println!(
        "Usage: sos_relay [options]\n\nTranslate Rocket League Stats API events into SOS-style websocket events and relay them to an outbound websocket endpoint.\n\nSource (RL Stats API):\n  --ini <path>           Optional DefaultStatsAPI.ini path\n  --rl-host <host>       Source host (alias: --host, default 127.0.0.1)\n  --rl-port <port>       Source port (alias: --port, default 49123)\n\nDestination (overlay websocket):\n  --ws-host <value>      Destination host or URL (e.g. 127.0.0.1, ws://host:9000/path, https://host/ws)\n  --ws-port <port>       Destination websocket port override\n  --ws-path <path>       Destination websocket path override\n\nRuntime:\n  -d, --debug            Enable verbose relay debug logs\n  --reconnect-ms <ms>    Reconnect delay for RL and websocket retries (default 500)\n  --max-events <n>       Stop after relaying n SOS messages\n\nExamples:\n  cargo run --bin sos_relay\n  cargo run --bin sos_relay -- --debug --ws-host 10.0.0.42 --ws-port 49122\n  cargo run --bin sos_relay -- --ws-host https://overlay.rscna.com/ws\n  cargo run --bin sos_relay -- --host 127.0.0.1 --port 49123 --ws-host 192.168.1.50 --ws-port 8080 --ws-path /sos"
    );
}

#[cfg(test)]
mod tests {
    use super::{build_ws_target, normalize_ws_path};

    #[test]
    fn builds_ws_url_from_bare_host_defaults() {
        let target = build_ws_target("127.0.0.1", None, None).unwrap();
        assert_eq!(target.to_url(), "ws://127.0.0.1:49122/");
    }

    #[test]
    fn maps_https_to_wss_and_keeps_path() {
        let target =
            build_ws_target("https://overlay.rscna.com/ws", None, None)
                .unwrap();
        assert_eq!(target.to_url(), "wss://overlay.rscna.com:443/ws");
    }

    #[test]
    fn explicit_overrides_take_precedence() {
        let target = build_ws_target(
            "https://overlay.rscna.com/ws/",
            Some(80),
            Some("/ws"),
        )
        .unwrap();
        assert_eq!(target.to_url(), "wss://overlay.rscna.com:80/ws");
    }

    #[test]
    fn normalizes_ws_path_values() {
        assert_eq!(normalize_ws_path(""), "/");
        assert_eq!(normalize_ws_path("/"), "/");
        assert_eq!(normalize_ws_path("ws"), "/ws");
        assert_eq!(normalize_ws_path("/ws"), "/ws");
    }
}
