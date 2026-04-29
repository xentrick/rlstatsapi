use std::collections::HashSet;
use std::io::{self, BufRead};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rlstatsapi::{
    ClientOptions, EventFilter, EventKind, RocketLeagueStatsClient, StatsEvent,
    parse_stats_event, stats_event_name, stats_event_to_value,
};

#[derive(Debug, Clone)]
struct CliOptions {
    ini_path: Option<PathBuf>,
    host: Option<String>,
    port: Option<u16>,
    event_kinds: HashSet<EventKind>,
    player_name: Option<String>,
    player_primary_id: Option<String>,
    team_num: Option<i64>,
    match_guid: Option<String>,
    output_format: OutputFormat,
    input_mode: InputMode,
    max_events: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
enum OutputFormat {
    Pretty,
    Compact,
    Json,
}

#[derive(Debug, Clone, Copy)]
enum InputMode {
    Stream,
    Stdin { strict: bool },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = parse_args()?;
    let filter = build_event_filter(&cli);

    match cli.input_mode {
        InputMode::Stream => run_stream_mode(&cli, &filter).await,
        InputMode::Stdin { strict } => run_stdin_mode(&cli, &filter, strict),
    }
}

async fn run_stream_mode(
    cli: &CliOptions,
    filter: &EventFilter,
) -> Result<(), Box<dyn std::error::Error>> {
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
        "Listening on {} (format={}, filters={})",
        client.connection().socket_address(),
        output_format_name(cli.output_format),
        describe_filters(cli)
    );

    let mut emitted = 0usize;

    loop {
        match client.next_event().await {
            Ok(Some(event)) => {
                if filter.matches(&event) {
                    print_event(&event, cli.output_format)?;
                    emitted += 1;
                    if cli.max_events.is_some_and(|max| emitted >= max) {
                        return Ok(());
                    }
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

fn run_stdin_mode(
    cli: &CliOptions,
    filter: &EventFilter,
    strict: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let mut emitted = 0usize;

    for (line_number, line) in stdin.lock().lines().enumerate() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        match parse_stats_event(trimmed) {
            Ok(event) => {
                if filter.matches(&event) {
                    print_event(&event, cli.output_format)?;
                    emitted += 1;
                    if cli.max_events.is_some_and(|max| emitted >= max) {
                        return Ok(());
                    }
                }
            }
            Err(error) => {
                if strict {
                    return Err(format!(
                        "stdin parse error at line {}: {}",
                        line_number + 1,
                        error
                    )
                    .into());
                }

                eprintln!(
                    "warning: skipped invalid event at line {}: {}",
                    line_number + 1,
                    error
                );
            }
        }
    }

    Ok(())
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

fn build_event_filter(cli: &CliOptions) -> EventFilter {
    let mut filter = EventFilter::new();

    if !cli.event_kinds.is_empty() {
        filter = filter.include_kinds(cli.event_kinds.iter().copied());
    }
    if let Some(player_name) = &cli.player_name {
        filter = filter.with_player_name(player_name.clone());
    }
    if let Some(primary_id) = &cli.player_primary_id {
        filter = filter.with_player_primary_id(primary_id.clone());
    }
    if let Some(team_num) = cli.team_num {
        filter = filter.with_team_num(team_num);
    }
    if let Some(match_guid) = &cli.match_guid {
        filter = filter.with_match_guid(match_guid.clone());
    }

    filter
}

fn print_event(
    event: &StatsEvent,
    output_format: OutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    match output_format {
        OutputFormat::Pretty => {
            print_pretty_event(event);
            Ok(())
        }
        OutputFormat::Compact => {
            print_compact_event(event);
            Ok(())
        }
        OutputFormat::Json => {
            let value = stats_event_to_value(event)?;
            println!("{}", serde_json::to_string(&value)?);
            Ok(())
        }
    }
}

fn print_pretty_event(event: &StatsEvent) {
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
                frame, time_left, blue, orange, players, ball_speed
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
            println!("[{ts}] UNKNOWN event={} data={}", data.event, data.data);
        }
        other => {
            println!("[{ts}] {}", stats_event_name(other));
        }
    }
}

fn print_compact_event(event: &StatsEvent) {
    let ts = timestamp();
    match event {
        StatsEvent::GoalScored(data) => {
            println!(
                "[{ts}] GOAL team={} scorer={} match={}",
                data.scorer.team_num,
                data.scorer.name,
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
        StatsEvent::UpdateState(data) => {
            println!(
                "[{ts}] UPDATE frame={} time={} players={}",
                data.game.frame.unwrap_or(-1),
                data.game.time_seconds.unwrap_or(-1),
                data.players.len()
            );
        }
        other => {
            println!("[{ts}] {}", stats_event_name(other));
        }
    }
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
    let mut event_kinds = HashSet::new();
    let mut player_name = None;
    let mut player_primary_id = None;
    let mut team_num = None;
    let mut match_guid = None;
    let mut output_format = OutputFormat::Pretty;
    let mut input_mode = InputMode::Stream;
    let mut max_events = None;

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
            "--event" => {
                let value = args.next().ok_or("--event requires a value")?;
                parse_event_filter_value(&value, &mut event_kinds)?;
            }
            "--player" => {
                let value = args.next().ok_or("--player requires a value")?;
                player_name = Some(value);
            }
            "--player-id" => {
                let value =
                    args.next().ok_or("--player-id requires a value")?;
                player_primary_id = Some(value);
            }
            "--team" => {
                let value = args.next().ok_or("--team requires a value")?;
                team_num = Some(value.parse::<i64>()?);
            }
            "--match" => {
                let value = args.next().ok_or("--match requires a value")?;
                match_guid = Some(value);
            }
            "--format" => {
                let value = args.next().ok_or("--format requires a value")?;
                output_format = parse_output_format(&value)?;
            }
            "--stdin" => {
                input_mode = InputMode::Stdin { strict: false };
            }
            "--strict-stdin" => {
                input_mode = InputMode::Stdin { strict: true };
            }
            "--max-events" => {
                let value =
                    args.next().ok_or("--max-events requires a value")?;
                max_events = Some(value.parse::<usize>()?);
            }
            "--list-events" => {
                print_event_list();
                std::process::exit(0);
            }
            "--list-formats" => {
                print_format_list();
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
        event_kinds,
        player_name,
        player_primary_id,
        team_num,
        match_guid,
        output_format,
        input_mode,
        max_events,
    })
}

fn parse_event_filter_value(
    value: &str,
    filters: &mut HashSet<EventKind>,
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

        let event_kind = event_kind_from_token(&normalized).ok_or_else(|| {
            format!(
                "unknown event filter '{normalized}'. Use --list-events to see valid values."
            )
        })?;
        filters.insert(event_kind);
    }

    Ok(())
}

fn parse_output_format(
    value: &str,
) -> Result<OutputFormat, Box<dyn std::error::Error>> {
    match value.to_ascii_lowercase().as_str() {
        "pretty" => Ok(OutputFormat::Pretty),
        "compact" => Ok(OutputFormat::Compact),
        "json" => Ok(OutputFormat::Json),
        other => Err(format!(
            "unknown format '{other}'. Use --list-formats to see valid values."
        )
        .into()),
    }
}

fn output_format_name(value: OutputFormat) -> &'static str {
    match value {
        OutputFormat::Pretty => "pretty",
        OutputFormat::Compact => "compact",
        OutputFormat::Json => "json",
    }
}

fn describe_filters(cli: &CliOptions) -> String {
    let mut parts = Vec::new();

    if !cli.event_kinds.is_empty() {
        let mut kinds = cli
            .event_kinds
            .iter()
            .copied()
            .map(event_kind_name)
            .collect::<Vec<_>>();
        kinds.sort_unstable();
        parts.push(format!("events={}", kinds.join(",")));
    }
    if let Some(name) = &cli.player_name {
        parts.push(format!("player={name}"));
    }
    if let Some(primary_id) = &cli.player_primary_id {
        parts.push(format!("player_id={primary_id}"));
    }
    if let Some(team_num) = cli.team_num {
        parts.push(format!("team={team_num}"));
    }
    if let Some(match_guid) = &cli.match_guid {
        parts.push(format!("match={match_guid}"));
    }

    if parts.is_empty() {
        "all".to_string()
    } else {
        parts.join(" ")
    }
}

fn event_kind_from_token(token: &str) -> Option<EventKind> {
    match token {
        "update_state" | "update" | "state" | "tick" => {
            Some(EventKind::UpdateState)
        }
        "ball_hit" | "ballhit" => Some(EventKind::BallHit),
        "clock_updated_seconds" | "clock" => {
            Some(EventKind::ClockUpdatedSeconds)
        }
        "countdown_begin" | "countdown" => Some(EventKind::CountdownBegin),
        "crossbar_hit" | "crossbar" => Some(EventKind::CrossbarHit),
        "goal_replay_end" => Some(EventKind::GoalReplayEnd),
        "goal_replay_start" => Some(EventKind::GoalReplayStart),
        "goal_replay_will_end" => Some(EventKind::GoalReplayWillEnd),
        "goal_scored" | "goal" => Some(EventKind::GoalScored),
        "match_created" => Some(EventKind::MatchCreated),
        "match_initialized" => Some(EventKind::MatchInitialized),
        "match_destroyed" => Some(EventKind::MatchDestroyed),
        "match_ended" | "ended" => Some(EventKind::MatchEnded),
        "match_paused" | "paused" | "pause" => Some(EventKind::MatchPaused),
        "match_unpaused" | "unpaused" | "unpause" => {
            Some(EventKind::MatchUnpaused)
        }
        "podium_start" => Some(EventKind::PodiumStart),
        "replay_created" => Some(EventKind::ReplayCreated),
        "round_started" | "round" => Some(EventKind::RoundStarted),
        "statfeed_event" | "statfeed" => Some(EventKind::StatfeedEvent),
        "unknown" => Some(EventKind::Unknown),
        _ => None,
    }
}

fn event_kind_name(kind: EventKind) -> &'static str {
    match kind {
        EventKind::UpdateState => "update_state",
        EventKind::BallHit => "ball_hit",
        EventKind::ClockUpdatedSeconds => "clock_updated_seconds",
        EventKind::CountdownBegin => "countdown_begin",
        EventKind::CrossbarHit => "crossbar_hit",
        EventKind::GoalReplayEnd => "goal_replay_end",
        EventKind::GoalReplayStart => "goal_replay_start",
        EventKind::GoalReplayWillEnd => "goal_replay_will_end",
        EventKind::GoalScored => "goal_scored",
        EventKind::MatchCreated => "match_created",
        EventKind::MatchInitialized => "match_initialized",
        EventKind::MatchDestroyed => "match_destroyed",
        EventKind::MatchEnded => "match_ended",
        EventKind::MatchPaused => "match_paused",
        EventKind::MatchUnpaused => "match_unpaused",
        EventKind::PodiumStart => "podium_start",
        EventKind::ReplayCreated => "replay_created",
        EventKind::RoundStarted => "round_started",
        EventKind::StatfeedEvent => "statfeed_event",
        EventKind::Unknown => "unknown",
    }
}

fn print_help() {
    println!(
        "Usage: pretty_events [options]\n\nParse and filter Rocket League Stats API events from stream or stdin.\n\nInput:\n  --stdin                Read JSON events from stdin (one JSON object per line)\n  --strict-stdin         Same as --stdin, but fail on first invalid JSON line\n\nConnection (stream mode):\n  --ini <path>           Optional DefaultStatsAPI.ini path\n  --host <host>          Override host (default 127.0.0.1)\n  --port <port>          Override port (default 49123)\n\nFiltering:\n  --event <a,b,c>        Event kind filter list (use --list-events)\n  --player <name>        Match events involving player name\n  --player-id <id>       Match events involving player primary id\n  --team <num>           Match events involving team number\n  --match <guid>         Match events for specific match guid\n\nOutput:\n  --format <kind>        pretty | compact | json\n  --max-events <n>       Stop after emitting n matching events\n  --list-events          Print available event names\n  --list-formats         Print available output formats\n\nExamples:\n  pretty_events --event goal,match_ended\n  pretty_events --event update_state --player SomePlayer --format compact\n  cat events.jsonl | pretty_events --stdin --event goal --format json"
    );
}

fn print_event_list() {
    let events = [
        EventKind::UpdateState,
        EventKind::BallHit,
        EventKind::ClockUpdatedSeconds,
        EventKind::CountdownBegin,
        EventKind::CrossbarHit,
        EventKind::GoalReplayEnd,
        EventKind::GoalReplayStart,
        EventKind::GoalReplayWillEnd,
        EventKind::GoalScored,
        EventKind::MatchCreated,
        EventKind::MatchInitialized,
        EventKind::MatchDestroyed,
        EventKind::MatchEnded,
        EventKind::MatchPaused,
        EventKind::MatchUnpaused,
        EventKind::PodiumStart,
        EventKind::ReplayCreated,
        EventKind::RoundStarted,
        EventKind::StatfeedEvent,
        EventKind::Unknown,
    ];

    println!("Available event filters:");
    for event in events {
        println!("  {}", event_kind_name(event));
    }
    println!(
        "Aliases: goal, update, tick, clock, pause, unpause, round, statfeed, all"
    );
}

fn print_format_list() {
    println!("Available output formats:");
    println!("  pretty");
    println!("  compact");
    println!("  json");
}
