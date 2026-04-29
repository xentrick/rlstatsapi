use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use rlstatsapi::{
    ClientOptions, RocketLeagueStatsClient, translate_stats_event,
};
use tokio::net::{TcpListener, TcpStream};
#[cfg(unix)]
use tokio::signal::unix::{SignalKind, signal as unix_signal};
use tokio::sync::{broadcast, watch};
use tokio::time::sleep;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Clone)]
struct CliOptions {
    ini_path: Option<PathBuf>,
    rl_host: Option<String>,
    rl_port: Option<u16>,
    ws_host: String,
    ws_port: u16,
    reconnect_ms: u64,
    max_events: Option<usize>,
    debug: bool,
}

#[derive(Debug, Clone)]
enum ParseOutcome {
    Run(CliOptions),
    Help,
}

fn debug_log(cli: &CliOptions, message: impl AsRef<str>) {
    if cli.debug {
        eprintln!("[debug] {}", message.as_ref());
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = parse_args()?;

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

    let bind_address = format!("{}:{}", cli.ws_host, cli.ws_port);
    let listener = TcpListener::bind(&bind_address).await?;

    println!(
        "Broadcast source={} websocket=ws://{}:{}",
        client.connection().socket_address(),
        cli.ws_host,
        cli.ws_port,
    );

    let (outgoing_tx, _) = broadcast::channel::<String>(1024);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let client_count = Arc::new(AtomicUsize::new(0));

    let accept_handle = tokio::spawn(run_accept_loop(
        listener,
        outgoing_tx.clone(),
        shutdown_rx.clone(),
        client_count.clone(),
        cli.clone(),
    ));

    let mut relayed = 0usize;
    let shutdown_signal = wait_for_shutdown_signal();
    tokio::pin!(shutdown_signal);

    loop {
        tokio::select! {
            signal_name = &mut shutdown_signal => {
                eprintln!("Received {signal_name}; shutting down broadcast server...");
                let _ = shutdown_tx.send(true);
                break;
            }
            next_event = client.next_event() => {
                match next_event {
                    Ok(Some(event)) => {
                        for envelope in translate_stats_event(&event)? {
                            let payload = serde_json::to_string(&envelope)?;

                            match outgoing_tx.send(payload) {
                                Ok(subscribers) => {
                                    debug_log(
                                        &cli,
                                        format!(
                                            "broadcast '{}' to {} websocket client(s)",
                                            envelope.event,
                                            subscribers,
                                        ),
                                    );
                                }
                                Err(_) => {
                                    debug_log(
                                        &cli,
                                        format!(
                                            "dropped '{}' because no websocket clients are connected",
                                            envelope.event,
                                        ),
                                    );
                                }
                            }

                            relayed += 1;
                            if cli.max_events.is_some_and(|max| relayed >= max) {
                                debug_log(
                                    &cli,
                                    format!("max-events reached ({}), stopping", relayed),
                                );
                                let _ = shutdown_tx.send(true);
                                if let Err(error) = accept_handle.await {
                                    eprintln!("accept loop join error: {error}");
                                }
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

    if let Err(error) = accept_handle.await {
        eprintln!("accept loop join error: {error}");
    }

    Ok(())
}

async fn run_accept_loop(
    listener: TcpListener,
    outgoing_tx: broadcast::Sender<String>,
    mut shutdown_rx: watch::Receiver<bool>,
    client_count: Arc<AtomicUsize>,
    cli: CliOptions,
) {
    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    break;
                }
            }
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, peer_addr)) => {
                        debug_log(
                            &cli,
                            format!("incoming websocket connection from {peer_addr}"),
                        );

                        let outgoing_rx = outgoing_tx.subscribe();
                        let shutdown_child = shutdown_rx.clone();
                        let client_count_child = client_count.clone();
                        let cli_child = cli.clone();

                        tokio::spawn(async move {
                            handle_client(
                                stream,
                                peer_addr.to_string(),
                                outgoing_rx,
                                shutdown_child,
                                client_count_child,
                                cli_child,
                            )
                            .await;
                        });
                    }
                    Err(error) => {
                        eprintln!("websocket accept error: {error}");
                        sleep(Duration::from_millis(cli.reconnect_ms)).await;
                    }
                }
            }
        }
    }
}

async fn handle_client(
    stream: TcpStream,
    peer: String,
    mut outgoing_rx: broadcast::Receiver<String>,
    mut shutdown_rx: watch::Receiver<bool>,
    client_count: Arc<AtomicUsize>,
    cli: CliOptions,
) {
    let ws_stream = match accept_async(stream).await {
        Ok(stream) => stream,
        Err(error) => {
            debug_log(
                &cli,
                format!("websocket handshake failed for {peer}: {error}"),
            );
            return;
        }
    };

    let connected = client_count.fetch_add(1, Ordering::SeqCst) + 1;
    println!("Overlay client connected ({peer}); clients={connected}");

    let (mut writer, mut reader) = ws_stream.split();

    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    break;
                }
            }
            outgoing = outgoing_rx.recv() => {
                match outgoing {
                    Ok(payload) => {
                        if let Err(error) = writer.send(Message::Text(payload.into())).await {
                            debug_log(&cli, format!("send error for {peer}: {error}"));
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        debug_log(
                            &cli,
                            format!("client {peer} lagged and skipped {skipped} payload(s)"),
                        );
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
            inbound = reader.next() => {
                match inbound {
                    Some(Ok(Message::Ping(payload))) => {
                        if let Err(error) = writer.send(Message::Pong(payload)).await {
                            debug_log(&cli, format!("failed pong to {peer}: {error}"));
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => {
                        debug_log(&cli, format!("client {peer} receive error: {error}"));
                        break;
                    }
                    None => {
                        break;
                    }
                }
            }
        }
    }

    let _ = writer.close().await;

    let remaining = client_count
        .fetch_sub(1, Ordering::SeqCst)
        .saturating_sub(1);
    println!("Overlay client disconnected ({peer}); clients={remaining}");
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

async fn reconnect_rl_with_backoff(
    client: &mut RocketLeagueStatsClient,
    cli: &CliOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut last_error: Option<Box<dyn std::error::Error>> = None;

    for attempt in 1..=10 {
        debug_log(
            cli,
            format!("attempting RL reconnect (attempt {attempt}/10)"),
        );

        match client.reconnect().await {
            Ok(()) => {
                debug_log(
                    cli,
                    format!(
                        "RL reconnect succeeded; source={}",
                        client.connection().socket_address()
                    ),
                );
                return Ok(());
            }
            Err(error) => {
                debug_log(
                    cli,
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

fn parse_args() -> Result<CliOptions, Box<dyn std::error::Error>> {
    match parse_args_from(std::env::args().skip(1))? {
        ParseOutcome::Run(cli) => Ok(cli),
        ParseOutcome::Help => {
            print_help();
            std::process::exit(0);
        }
    }
}

fn parse_args_from<I>(
    args: I,
) -> Result<ParseOutcome, Box<dyn std::error::Error>>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();

    let mut ini_path = None;
    let mut rl_host = None;
    let mut rl_port = None;
    let mut ws_host = "0.0.0.0".to_string();
    let mut ws_port = 49122u16;
    let mut reconnect_ms = 500u64;
    let mut max_events = None;
    let mut debug = false;

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
                ws_port = value.parse::<u16>()?;
            }
            "--reconnect-ms" => {
                let value =
                    args.next().ok_or("--reconnect-ms requires a value")?;
                reconnect_ms = value.parse::<u64>()?;
            }
            "--max-events" => {
                let value =
                    args.next().ok_or("--max-events requires a value")?;
                max_events = Some(value.parse::<usize>()?);
            }
            "-d" | "--debug" => {
                debug = true;
            }
            "-h" | "--help" => {
                return Ok(ParseOutcome::Help);
            }
            other => {
                return Err(format!("Unknown argument: {other}").into());
            }
        }
    }

    Ok(ParseOutcome::Run(CliOptions {
        ini_path,
        rl_host,
        rl_port,
        ws_host,
        ws_port,
        reconnect_ms,
        max_events,
        debug,
    }))
}

fn print_help() {
    println!(
        "Usage: sos_broadcast [options]\n\nTranslate Rocket League Stats API events into SOS-style websocket events and host them for local/network websocket clients.\n\nSource (RL Stats API):\n  --ini <path>           Optional DefaultStatsAPI.ini path\n  --rl-host <host>       Source host (alias: --host, default 127.0.0.1)\n  --rl-port <port>       Source port (alias: --port, default 49123)\n\nWebsocket server:\n  --ws-host <host>       Bind host for websocket server (default 0.0.0.0)\n  --ws-port <port>       Bind port for websocket server (default 49122)\n\nRuntime:\n  -d, --debug            Enable verbose debug logs\n  --reconnect-ms <ms>    Reconnect delay for RL retries (default 500)\n  --max-events <n>       Stop after broadcasting n SOS messages\n\nExamples:\n  cargo run --bin sos_broadcast\n  cargo run --bin sos_broadcast -- --ws-host 0.0.0.0 --ws-port 49122\n  cargo run --bin sos_broadcast -- --host 127.0.0.1 --port 49123 --debug\n\nClients can connect to ws://localhost:49122"
    );
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{ParseOutcome, parse_args_from};

    #[test]
    fn parse_args_uses_expected_defaults() {
        let parsed =
            parse_args_from(Vec::<String>::new()).expect("parse defaults");

        let ParseOutcome::Run(cli) = parsed else {
            panic!("expected runnable options");
        };

        assert_eq!(cli.rl_host, None);
        assert_eq!(cli.rl_port, None);
        assert_eq!(cli.ws_host, "0.0.0.0");
        assert_eq!(cli.ws_port, 49122);
        assert_eq!(cli.reconnect_ms, 500);
        assert_eq!(cli.max_events, None);
        assert!(!cli.debug);
    }

    #[test]
    fn parse_args_accepts_aliases_and_overrides() {
        let parsed = parse_args_from(vec![
            "--ini".to_string(),
            "/tmp/DefaultStatsAPI.ini".to_string(),
            "--host".to_string(),
            "10.0.0.1".to_string(),
            "--port".to_string(),
            "49124".to_string(),
            "--ws-host".to_string(),
            "127.0.0.1".to_string(),
            "--ws-port".to_string(),
            "9001".to_string(),
            "--reconnect-ms".to_string(),
            "750".to_string(),
            "--max-events".to_string(),
            "12".to_string(),
            "--debug".to_string(),
        ])
        .expect("parse override args");

        let ParseOutcome::Run(cli) = parsed else {
            panic!("expected runnable options");
        };

        assert_eq!(
            cli.ini_path,
            Some(PathBuf::from("/tmp/DefaultStatsAPI.ini"))
        );
        assert_eq!(cli.rl_host.as_deref(), Some("10.0.0.1"));
        assert_eq!(cli.rl_port, Some(49124));
        assert_eq!(cli.ws_host, "127.0.0.1");
        assert_eq!(cli.ws_port, 9001);
        assert_eq!(cli.reconnect_ms, 750);
        assert_eq!(cli.max_events, Some(12));
        assert!(cli.debug);
    }

    #[test]
    fn parse_args_recognizes_help() {
        let parsed =
            parse_args_from(vec!["--help".to_string()]).expect("parse help");
        assert!(matches!(parsed, ParseOutcome::Help));
    }
}
