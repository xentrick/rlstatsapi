use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use rlstatsapi::{ClientOptions, RocketLeagueStatsClient, StatsEvent};

#[derive(Debug, Clone)]
struct CliOptions {
    ini_path: Option<PathBuf>,
    host: Option<String>,
    port: Option<u16>,
    refresh_ms: u64,
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

    if let Some(host) = cli.host.clone() {
        options.host = host;
    }
    if let Some(port) = cli.port {
        options.port_override = Some(port);
    }

    let mut client = RocketLeagueStatsClient::connect_with_retry(
        options,
        20,
        Duration::from_millis(250),
    )
    .await?;

    let mut last_render =
        Instant::now() - Duration::from_millis(cli.refresh_ms);

    println!(
        "Tracking players from {} (refresh={}ms)",
        client.connection().socket_address(),
        cli.refresh_ms
    );

    loop {
        match client.next_event().await {
            Ok(Some(StatsEvent::UpdateState(state))) => {
                if last_render.elapsed()
                    >= Duration::from_millis(cli.refresh_ms)
                {
                    render_state(&state)?;
                    last_render = Instant::now();
                }
            }
            Ok(Some(_)) => {}
            Ok(None) => {
                eprintln!("connection closed; attempting reconnect...");
                reconnect_with_backoff(&mut client).await?;
            }
            Err(error) => {
                eprintln!("stream error: {error}; attempting reconnect...");
                reconnect_with_backoff(&mut client).await?;
            }
        }
    }
}

async fn reconnect_with_backoff(
    client: &mut RocketLeagueStatsClient,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut last_error: Option<Box<dyn std::error::Error>> = None;

    for _ in 0..10 {
        match client.reconnect().await {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(Box::new(error));
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }

    match last_error {
        Some(error) => Err(error),
        None => Err("reconnect failed".into()),
    }
}

fn render_state(
    data: &rlstatsapi::events::UpdateStateData,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut players = data.players.iter().collect::<Vec<_>>();
    players.sort_by(|left, right| {
        let left_team = left.team_num.unwrap_or(-1);
        let right_team = right.team_num.unwrap_or(-1);
        left_team.cmp(&right_team).then_with(|| {
            left.name
                .as_deref()
                .unwrap_or("")
                .cmp(right.name.as_deref().unwrap_or(""))
        })
    });

    let blue = team_score(data, 0).unwrap_or(-1);
    let orange = team_score(data, 1).unwrap_or(-1);

    let mut out = io::stdout().lock();
    write!(out, "\x1b[2J\x1b[H")?;

    writeln!(
        out,
        "Match={}  Frame={}  Time={}s  Score={}-{}  Players={}",
        data.match_guid.as_deref().unwrap_or("-"),
        data.game.frame.unwrap_or(-1),
        data.game.time_seconds.unwrap_or(-1),
        blue,
        orange,
        players.len()
    )?;
    writeln!(
        out,
        "{:<4} {:<18} {:<7} {:>6} {:>5} {:>5} {:>5} {:>6} {:>6} {:>6} {:>7}",
        "Team",
        "Player",
        "Primary",
        "Score",
        "G",
        "A",
        "S",
        "Shots",
        "Touch",
        "Boost",
        "Speed"
    )?;
    writeln!(
        out,
        "{:-<4} {:-<18} {:-<7} {:-<6} {:-<5} {:-<5} {:-<5} {:-<6} {:-<6} {:-<6} {:-<7}",
        "", "", "", "", "", "", "", "", "", "", ""
    )?;

    for player in players {
        let primary_short = player
            .primary_id
            .as_deref()
            .map(short_primary_id)
            .unwrap_or("-");
        writeln!(
            out,
            "{:<4} {:<18} {:<7} {:>6} {:>5} {:>5} {:>5} {:>6} {:>6} {:>6} {:>7}",
            fmt_opt_i64(player.team_num),
            player.name.as_deref().unwrap_or("-"),
            primary_short,
            fmt_opt_i64(player.score),
            fmt_opt_i64(player.goals),
            fmt_opt_i64(player.assists),
            fmt_opt_i64(player.saves),
            fmt_opt_i64(player.shots),
            fmt_opt_i64(player.touches),
            fmt_opt_i64(player.effective_boost()),
            fmt_opt_f64(player.effective_speed())
        )?;
    }

    out.flush()?;
    Ok(())
}

fn short_primary_id(value: &str) -> &str {
    value.split('|').next_back().unwrap_or(value)
}

fn fmt_opt_i64(value: Option<i64>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn fmt_opt_f64(value: Option<f64>) -> String {
    value
        .map(|v| format!("{v:.0}"))
        .unwrap_or_else(|| "-".to_string())
}

fn team_score(
    data: &rlstatsapi::events::UpdateStateData,
    team_num: i64,
) -> Option<i64> {
    data.game
        .teams
        .iter()
        .find(|team| team.team_num == Some(team_num))
        .and_then(|team| team.score)
}

fn parse_args() -> Result<CliOptions, Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);

    let mut ini_path = None;
    let mut host = None;
    let mut port = None;
    let mut refresh_ms = 200u64;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--ini" => {
                let value = args.next().ok_or("--ini requires a file path")?;
                ini_path = Some(PathBuf::from(value));
            }
            "--host" => {
                let value = args.next().ok_or("--host requires a value")?;
                host = Some(value);
            }
            "--port" => {
                let value = args.next().ok_or("--port requires a value")?;
                port = Some(value.parse::<u16>()?);
            }
            "--refresh-ms" => {
                let value =
                    args.next().ok_or("--refresh-ms requires a value")?;
                refresh_ms = value.parse::<u64>()?;
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

    Ok(CliOptions {
        ini_path,
        host,
        port,
        refresh_ms,
    })
}

fn print_help() {
    println!(
        "Usage: player_board [--ini <path>] [--host <host>] [--port <port>] [--refresh-ms <n>]\n\nContinuously renders all players in-place using UpdateState events (no scrolling spam).\n\nExamples:\n  player_board\n  player_board --refresh-ms 100\n  player_board --ini /path/to/DefaultStatsAPI.ini"
    );
}
