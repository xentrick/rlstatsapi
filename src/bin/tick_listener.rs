use std::path::PathBuf;

use rlstatsapi::{ClientOptions, RocketLeagueStatsClient, StatsEvent};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut options = ClientOptions::default();

    if let Some(path) = parse_ini_arg()? {
        options.stats_api_ini_path = Some(path);
    } else {
        options.auto_enable_packet_rate = false;
    }

    let mut client = RocketLeagueStatsClient::connect(options).await?;

    println!(
        "Listening on {} (packet_send_rate={}, ini_mutated={})",
        client.connection().socket_address(),
        client.connection().packet_send_rate,
        client.connection().ini_mutated
    );

    while let Some(event) = client.next_event().await? {
        if let StatsEvent::UpdateState(data) = event {
            let blue_score = team_score(&data, 0).unwrap_or(-1);
            let orange_score = team_score(&data, 1).unwrap_or(-1);
            let frame = data.game.frame.unwrap_or(-1);
            let time_seconds = data.game.time_seconds.unwrap_or(-1);
            let overtime = data.game.b_overtime.unwrap_or(false);
            let player_count = data.players.len();
            let ball_speed = data
                .game
                .ball
                .as_ref()
                .and_then(|ball| ball.speed)
                .unwrap_or(0.0);

            println!(
                "tick frame={} time={}s ot={} score={}-{} players={} ball_speed={:.1}",
                frame,
                time_seconds,
                overtime,
                blue_score,
                orange_score,
                player_count,
                ball_speed
            );
        }
    }

    Ok(())
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
                    "Usage: tick_listener [--ini <path>]\n\nIf --ini is omitted, uses 127.0.0.1:49123 without INI edits."
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
