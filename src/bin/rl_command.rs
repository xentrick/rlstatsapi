use std::time::Duration;

use rlstatsapi::{
    ClientOptions, DEFAULT_PORT, RocketLeagueStatsClient, StatsCommand,
    stats_event_name,
};
use serde_json::Value;
use tokio::time::{Instant, timeout_at};

struct CliOptions {
    host: String,
    port: u16,
    watch_seconds: u64,
    command: StatsCommand,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            rlstatsapi::console::report_fatal_error(error.as_ref());
            rlstatsapi::console::pause_before_exit();
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = parse_args()?;

    let options = ClientOptions {
        host: cli.host.clone(),
        port_override: Some(cli.port),
        auto_enable_packet_rate: false,
        ..ClientOptions::default()
    };
    let mut client =
        RocketLeagueStatsClient::connect(options)
            .await
            .map_err(|error| {
                rlstatsapi::console::explain_connection_failure(
                    &cli.host, cli.port, &error,
                )
            })?;
    println!("Connected to {}", client.connection().socket_address());

    println!("Sending {}", cli.command.to_json()?);
    client.send_command(&cli.command).await?;

    // Echo non-tick events for a few seconds so the effect (e.g. MatchPaused)
    // can be confirmed against the live game.
    let deadline = Instant::now() + Duration::from_secs(cli.watch_seconds);
    while let Ok(next) = timeout_at(deadline, client.next_event()).await {
        match next? {
            Some(event) => {
                let name = stats_event_name(&event);
                if name != "UpdateState" {
                    println!("  <- {name}");
                }
            }
            None => break,
        }
    }

    Ok(())
}

fn parse_args() -> Result<CliOptions, String> {
    let mut args = std::env::args().skip(1);
    let mut host = "127.0.0.1".to_string();
    let mut port = DEFAULT_PORT;
    let mut watch_seconds = 3;
    let mut raw_command = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--host" => {
                host = args
                    .next()
                    .ok_or_else(|| "--host requires a value".to_string())?;
            }
            "--port" => {
                port = args
                    .next()
                    .ok_or_else(|| "--port requires a value".to_string())?
                    .parse()
                    .map_err(|_| "--port must be a number".to_string())?;
            }
            "--watch" => {
                watch_seconds = args
                    .next()
                    .ok_or_else(|| "--watch requires seconds".to_string())?
                    .parse()
                    .map_err(|_| "--watch must be a number".to_string())?;
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other if raw_command.is_none() && !other.starts_with("--") => {
                raw_command = Some(other.to_string());
            }
            other => {
                return Err(format!("Unknown argument: {other}"));
            }
        }
    }

    let raw_command =
        raw_command.ok_or_else(|| "missing command JSON".to_string())?;
    let value: Value = serde_json::from_str(&raw_command)
        .map_err(|error| format!("command is not valid JSON: {error}"))?;
    let command =
        StatsCommand::from_value(&value).map_err(|error| error.to_string())?;

    Ok(CliOptions {
        host,
        port,
        watch_seconds,
        command,
    })
}

fn print_help() {
    println!(
        "Usage: rl_command [--host <host>] [--port <port>] [--watch <secs>] '<json>'\n\nSend one Stats API command to the game, then print non-tick events for --watch seconds (default 3).\n\nCommands: {}\n\nExamples:\n  rl_command '{{\"Command\":\"SetMatchPaused\",\"Data\":{{\"bPaused\":true}}}}'\n  rl_command '{{\"Command\":\"ChangePOV\",\"Data\":{{\"Focus\":\"Ball\",\"Perspective\":\"SoftAttach\"}}}}'",
        StatsCommand::NAMES.join(", ")
    );
}
