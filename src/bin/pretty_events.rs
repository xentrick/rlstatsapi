use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rlstatsapi::{ClientOptions, RocketLeagueStatsClient, StatsEvent};

#[derive(Debug, Clone)]
struct CliOptions {
    ini_path: Option<PathBuf>,
    host: Option<String>,
    port: Option<u16>,
    filters: HashSet<EventType>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum EventType {
    UpdateState,
    BallHit,
    ClockUpdatedSeconds,
    CountdownBegin,
    CrossbarHit,
    GoalReplayEnd,
    GoalReplayStart,
    GoalReplayWillEnd,
    GoalScored,
    MatchCreated,
    MatchInitialized,
    MatchDestroyed,
    MatchEnded,
    MatchPaused,
    MatchUnpaused,
    PodiumStart,
    ReplayCreated,
    RoundStarted,
    StatfeedEvent,
    Unknown,
}

impl EventType {
    fn from_token(token: &str) -> Option<Self> {
        match token {
            "update_state" | "update" | "state" | "tick" => {
                Some(Self::UpdateState)
            }
            "ball_hit" | "ballhit" => Some(Self::BallHit),
            "clock_updated_seconds" | "clock" => {
                Some(Self::ClockUpdatedSeconds)
            }
            "countdown_begin" | "countdown" => Some(Self::CountdownBegin),
            "crossbar_hit" | "crossbar" => Some(Self::CrossbarHit),
            "goal_replay_end" => Some(Self::GoalReplayEnd),
            "goal_replay_start" => Some(Self::GoalReplayStart),
            "goal_replay_will_end" => Some(Self::GoalReplayWillEnd),
            "goal_scored" | "goal" => Some(Self::GoalScored),
            "match_created" => Some(Self::MatchCreated),
            "match_initialized" => Some(Self::MatchInitialized),
            "match_destroyed" => Some(Self::MatchDestroyed),
            "match_ended" | "ended" => Some(Self::MatchEnded),
            "match_paused" | "paused" | "pause" => Some(Self::MatchPaused),
            "match_unpaused" | "unpaused" | "unpause" => {
                Some(Self::MatchUnpaused)
            }
            "podium_start" => Some(Self::PodiumStart),
            "replay_created" => Some(Self::ReplayCreated),
            "round_started" | "round" => Some(Self::RoundStarted),
            "statfeed_event" | "statfeed" => Some(Self::StatfeedEvent),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }

    fn canonical_name(self) -> &'static str {
        match self {
            Self::UpdateState => "update_state",
            Self::BallHit => "ball_hit",
            Self::ClockUpdatedSeconds => "clock_updated_seconds",
            Self::CountdownBegin => "countdown_begin",
            Self::CrossbarHit => "crossbar_hit",
            Self::GoalReplayEnd => "goal_replay_end",
            Self::GoalReplayStart => "goal_replay_start",
            Self::GoalReplayWillEnd => "goal_replay_will_end",
            Self::GoalScored => "goal_scored",
            Self::MatchCreated => "match_created",
            Self::MatchInitialized => "match_initialized",
            Self::MatchDestroyed => "match_destroyed",
            Self::MatchEnded => "match_ended",
            Self::MatchPaused => "match_paused",
            Self::MatchUnpaused => "match_unpaused",
            Self::PodiumStart => "podium_start",
            Self::ReplayCreated => "replay_created",
            Self::RoundStarted => "round_started",
            Self::StatfeedEvent => "statfeed_event",
            Self::Unknown => "unknown",
        }
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

    println!(
        "Listening on {} (filters={})",
        client.connection().socket_address(),
        format_filters(&cli.filters)
    );

    loop {
        match client.next_event().await {
            Ok(Some(event)) => {
                if should_print(&cli.filters, &event) {
                    print_event(&event);
                }
            }
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

fn should_print(filters: &HashSet<EventType>, event: &StatsEvent) -> bool {
    if filters.is_empty() {
        return true;
    }

    filters.contains(&event_type(event))
}

fn event_type(event: &StatsEvent) -> EventType {
    match event {
        StatsEvent::UpdateState(_) => EventType::UpdateState,
        StatsEvent::BallHit(_) => EventType::BallHit,
        StatsEvent::ClockUpdatedSeconds(_) => EventType::ClockUpdatedSeconds,
        StatsEvent::CountdownBegin(_) => EventType::CountdownBegin,
        StatsEvent::CrossbarHit(_) => EventType::CrossbarHit,
        StatsEvent::GoalReplayEnd(_) => EventType::GoalReplayEnd,
        StatsEvent::GoalReplayStart(_) => EventType::GoalReplayStart,
        StatsEvent::GoalReplayWillEnd(_) => EventType::GoalReplayWillEnd,
        StatsEvent::GoalScored(_) => EventType::GoalScored,
        StatsEvent::MatchCreated(_) => EventType::MatchCreated,
        StatsEvent::MatchInitialized(_) => EventType::MatchInitialized,
        StatsEvent::MatchDestroyed(_) => EventType::MatchDestroyed,
        StatsEvent::MatchEnded(_) => EventType::MatchEnded,
        StatsEvent::MatchPaused(_) => EventType::MatchPaused,
        StatsEvent::MatchUnpaused(_) => EventType::MatchUnpaused,
        StatsEvent::PodiumStart(_) => EventType::PodiumStart,
        StatsEvent::ReplayCreated(_) => EventType::ReplayCreated,
        StatsEvent::RoundStarted(_) => EventType::RoundStarted,
        StatsEvent::StatfeedEvent(_) => EventType::StatfeedEvent,
        StatsEvent::Unknown(_) => EventType::Unknown,
    }
}

fn format_filters(filters: &HashSet<EventType>) -> String {
    if filters.is_empty() {
        return "all".to_string();
    }

    let mut names = filters
        .iter()
        .map(|event| event.canonical_name())
        .collect::<Vec<_>>();
    names.sort_unstable();
    names.join(",")
}

fn print_event(event: &StatsEvent) {
    let ts = timestamp();

    match event {
        StatsEvent::GoalScored(data) => {
            let scorer = if data.scorer.name.is_empty() {
                "unknown"
            } else {
                data.scorer.name.as_str()
            };
            let assister = data
                .assister
                .as_ref()
                .map(|player| {
                    if player.name.is_empty() {
                        "unknown"
                    } else {
                        player.name.as_str()
                    }
                })
                .unwrap_or("-");
            let team = data.scorer.team_num;
            println!(
                "[{ts}] GOAL team={} scorer={} assister={} speed={:.1} time={:.1}s match={}",
                team,
                scorer,
                assister,
                data.goal_speed,
                data.goal_time,
                data.match_guid.as_deref().unwrap_or("-")
            );
        }
        StatsEvent::UpdateState(data) => {
            let blue = team_score(data, 0).unwrap_or(-1);
            let orange = team_score(data, 1).unwrap_or(-1);
            let time_left = data.game.time_seconds.unwrap_or(-1);
            let frame = data.game.frame.unwrap_or(-1);
            let players = data.players.len();
            let ball_speed = data
                .game
                .ball
                .as_ref()
                .and_then(|ball| ball.speed)
                .unwrap_or(0.0);
            println!(
                "[{ts}] UPDATE frame={} time={}s score={}-{} players={} ball_speed={:.1}",
                frame,
                time_left,
                blue,
                orange,
                players,
                ball_speed
            );
        }
        StatsEvent::ClockUpdatedSeconds(data) => {
            println!(
                "[{ts}] CLOCK time={}s overtime={} match={}",
                data.time_seconds,
                data.b_overtime,
                data.match_guid.as_deref().unwrap_or("-")
            );
        }
        StatsEvent::MatchEnded(data) => {
            println!(
                "[{ts}] MATCH_ENDED winner_team={} match={}",
                data.winner_team_num,
                data.match_guid.as_deref().unwrap_or("-")
            );
        }
        StatsEvent::StatfeedEvent(data) => {
            let secondary = data
                .secondary_target
                .as_ref()
                .map(|player| player.name.as_str())
                .unwrap_or("-");
            println!(
                "[{ts}] STATFEED type={} event={} main={} secondary={}",
                data.type_label,
                data.event_name,
                data.main_target.name,
                secondary
            );
        }
        StatsEvent::Unknown(data) => {
            println!(
                "[{ts}] UNKNOWN event={} data={}",
                data.event,
                data.data
            );
        }
        other => {
            println!("[{ts}] {}", event_label(other));
        }
    }
}

fn event_label(event: &StatsEvent) -> &'static str {
    match event {
        StatsEvent::UpdateState(_) => "UpdateState",
        StatsEvent::BallHit(_) => "BallHit",
        StatsEvent::ClockUpdatedSeconds(_) => "ClockUpdatedSeconds",
        StatsEvent::CountdownBegin(_) => "CountdownBegin",
        StatsEvent::CrossbarHit(_) => "CrossbarHit",
        StatsEvent::GoalReplayEnd(_) => "GoalReplayEnd",
        StatsEvent::GoalReplayStart(_) => "GoalReplayStart",
        StatsEvent::GoalReplayWillEnd(_) => "GoalReplayWillEnd",
        StatsEvent::GoalScored(_) => "GoalScored",
        StatsEvent::MatchCreated(_) => "MatchCreated",
        StatsEvent::MatchInitialized(_) => "MatchInitialized",
        StatsEvent::MatchDestroyed(_) => "MatchDestroyed",
        StatsEvent::MatchEnded(_) => "MatchEnded",
        StatsEvent::MatchPaused(_) => "MatchPaused",
        StatsEvent::MatchUnpaused(_) => "MatchUnpaused",
        StatsEvent::PodiumStart(_) => "PodiumStart",
        StatsEvent::ReplayCreated(_) => "ReplayCreated",
        StatsEvent::RoundStarted(_) => "RoundStarted",
        StatsEvent::StatfeedEvent(_) => "StatfeedEvent",
        StatsEvent::Unknown(_) => "Unknown",
    }
}

fn team_score(data: &rlstatsapi::events::UpdateStateData, team_num: i64) -> Option<i64> {
    data.game
        .teams
        .iter()
        .find(|team| team.team_num == Some(team_num))
        .and_then(|team| team.score)
}

fn timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0));

    format!("{}.{:03}", now.as_secs(), now.subsec_millis())
}

fn parse_args() -> Result<CliOptions, Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);

    let mut ini_path = None;
    let mut host = None;
    let mut port = None;
    let mut filters = HashSet::new();

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
                let parsed = value.parse::<u16>()?;
                port = Some(parsed);
            }
            "--event" => {
                let value = args.next().ok_or("--event requires a value")?;
                parse_filter_value(&value, &mut filters)?;
            }
            "--list-events" => {
                print_event_list();
                std::process::exit(0);
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
        filters,
    })
}

fn parse_filter_value(
    value: &str,
    filters: &mut HashSet<EventType>,
) -> Result<(), Box<dyn std::error::Error>> {
    for token in value.split(',') {
        let normalized = token.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            continue;
        }

        if normalized == "all" {
            filters.clear();
            continue;
        }

        let event = EventType::from_token(&normalized).ok_or_else(|| {
            format!(
                "unknown event filter '{normalized}'. Use --list-events to see valid values."
            )
        })?;
        filters.insert(event);
    }

    Ok(())
}

fn print_help() {
    println!(
        "Usage: pretty_events [--ini <path>] [--host <host>] [--port <port>] [--event <name[,name...]>]\n\nDisplay parsed events with human-friendly formatting and optional event filtering.\n\nExamples:\n  pretty_events\n  pretty_events --event goal\n  pretty_events --event goal,match_ended\n  pretty_events --host 127.0.0.1 --port 49123\n  pretty_events --ini /path/to/DefaultStatsAPI.ini --event update_state\n\nUse --list-events to print available filter names."
    );
}

fn print_event_list() {
    let events = [
        EventType::UpdateState,
        EventType::BallHit,
        EventType::ClockUpdatedSeconds,
        EventType::CountdownBegin,
        EventType::CrossbarHit,
        EventType::GoalReplayEnd,
        EventType::GoalReplayStart,
        EventType::GoalReplayWillEnd,
        EventType::GoalScored,
        EventType::MatchCreated,
        EventType::MatchInitialized,
        EventType::MatchDestroyed,
        EventType::MatchEnded,
        EventType::MatchPaused,
        EventType::MatchUnpaused,
        EventType::PodiumStart,
        EventType::ReplayCreated,
        EventType::RoundStarted,
        EventType::StatfeedEvent,
        EventType::Unknown,
    ];

    println!("Available event filters:");
    for event in events {
        println!("  {}", event.canonical_name());
    }
    println!("Aliases: goal, update, tick, clock, pause, unpause, round, statfeed, all");
}
