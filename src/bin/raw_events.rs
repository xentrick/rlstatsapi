use std::path::PathBuf;

use rlstatsapi::{ClientOptions, RocketLeagueStatsClient};

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
    let mut options = ClientOptions::default();

    if let Some(path) = parse_ini_arg()? {
        options.stats_api_ini_path = Some(path);
    } else {
        options.auto_enable_packet_rate = false;
    }

    let requested_host = options.host.clone();
    let requested_port = options.port_override.unwrap_or(49123);
    let mut client =
        RocketLeagueStatsClient::connect(options)
            .await
            .map_err(|error| {
                rlstatsapi::console::explain_connection_failure(
                    &requested_host,
                    requested_port,
                    &error,
                )
            })?;
    println!("Connected to {}", client.connection().socket_address());

    while let Some(event) = client.next_event().await? {
        println!("{event:?}");
    }

    Ok(())
}

fn parse_ini_arg() -> Result<Option<PathBuf>, String> {
    let mut args = std::env::args().skip(1);
    let mut ini_path = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--ini" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--ini requires a file path".to_string())?;
                ini_path = Some(PathBuf::from(value));
            }
            "-h" | "--help" => {
                println!(
                    "Usage: raw_events [--ini <path>]\n\nIf --ini is omitted, uses 127.0.0.1:49123 without INI edits."
                );
                std::process::exit(0);
            }
            other => {
                return Err(format!("Unknown argument: {other}"));
            }
        }
    }

    Ok(ini_path)
}
