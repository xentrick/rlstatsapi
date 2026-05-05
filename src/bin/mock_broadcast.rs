use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use rlstatsapi::SosEnvelope;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
#[cfg(unix)]
use tokio::signal::unix::{SignalKind, signal as unix_signal};
use tokio::sync::{broadcast, watch};
use tokio::time::{MissedTickBehavior, sleep};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

const MOCK_ENVELOPES_JSON: &str =
    include_str!("../../json/SOS/all_examples.json");

#[derive(Debug, Clone)]
struct CliOptions {
    ws_host: String,
    ws_port: u16,
    interval_ms: u64,
    max_events: Option<usize>,
    event_filters: Vec<String>,
    loop_events: bool,
    debug: bool,
}

#[derive(Debug, Clone)]
enum ParseOutcome {
    Run(CliOptions),
    Help,
    ListEvents,
}

#[derive(Debug, Deserialize)]
struct TemplateEnvelope {
    event: String,
    data: Value,
}

#[derive(Debug, Clone)]
struct MockPlayer {
    name: &'static str,
    id: &'static str,
    primary_id: &'static str,
    shortcut: &'static str,
    team: u64,
}

#[derive(Debug, Clone)]
struct UpdateStateRandomizer {
    rng_state: u64,
    tick: u64,
    remaining_seconds: i64,
    blue_score: u64,
    orange_score: u64,
    match_guid: String,
    players: Vec<MockPlayer>,
    player_identity_by_id: HashMap<&'static str, MockPlayer>,
    seen_player_ids: HashSet<&'static str>,
    last_player_payload_by_id: HashMap<&'static str, Value>,
}

impl UpdateStateRandomizer {
    fn new(seed: u64, match_guid: String) -> Self {
        let players = vec![
            MockPlayer {
                name: "BlueStriker",
                id: "BlueStriker_1",
                primary_id: "Steam|111|0",
                shortcut: "BS",
                team: 0,
            },
            MockPlayer {
                name: "BlueMid",
                id: "BlueMid_3",
                primary_id: "Steam|333|0",
                shortcut: "BM",
                team: 0,
            },
            MockPlayer {
                name: "BlueKeeper",
                id: "BlueKeeper_6",
                primary_id: "Steam|666|0",
                shortcut: "BK",
                team: 0,
            },
            MockPlayer {
                name: "OrangeStriker",
                id: "OrangeStriker_2",
                primary_id: "Epic|222|0",
                shortcut: "OS",
                team: 1,
            },
            MockPlayer {
                name: "OrangeMid",
                id: "OrangeMid_4",
                primary_id: "Epic|444|0",
                shortcut: "OM",
                team: 1,
            },
            MockPlayer {
                name: "OrangeKeeper",
                id: "OrangeKeeper_5",
                primary_id: "Epic|555|0",
                shortcut: "OK",
                team: 1,
            },
        ];

        let player_identity_by_id = players
            .iter()
            .cloned()
            .map(|player| (player.id, player))
            .collect::<HashMap<_, _>>();

        Self {
            rng_state: seed,
            tick: 0,
            remaining_seconds: 300,
            blue_score: 0,
            orange_score: 0,
            match_guid,
            players,
            player_identity_by_id,
            seen_player_ids: HashSet::new(),
            last_player_payload_by_id: HashMap::new(),
        }
    }

    fn identity_for(&self, player_id: &str) -> &MockPlayer {
        self.player_identity_by_id
            .get(player_id)
            .expect("player id must exist in canonical identity map")
    }

    fn next_u64(&mut self) -> u64 {
        // xorshift64* for lightweight pseudo-randomized mock values.
        let mut x = self.rng_state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng_state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn range_u64(&mut self, min: u64, max: u64) -> u64 {
        debug_assert!(min <= max);
        if min == max {
            return min;
        }
        min + (self.next_u64() % (max - min + 1))
    }

    fn range_i64(&mut self, min: i64, max: i64) -> i64 {
        debug_assert!(min <= max);
        if min == max {
            return min;
        }
        min + (self.next_u64() % (max - min + 1) as u64) as i64
    }

    fn chance(&mut self, numerator: u64, denominator: u64) -> bool {
        if denominator == 0 {
            return false;
        }
        self.next_u64() % denominator < numerator
    }

    fn next_update_state_payload(&mut self) -> Value {
        self.tick = self.tick.saturating_add(1);

        let drain = self.range_i64(1, 3);
        self.remaining_seconds -= drain;
        if self.remaining_seconds < 0 {
            self.remaining_seconds = 300;
            self.blue_score = 0;
            self.orange_score = 0;
        }

        if self.chance(1, 14) {
            self.blue_score = self.blue_score.saturating_add(1);
        }
        if self.chance(1, 14) {
            self.orange_score = self.orange_score.saturating_add(1);
        }

        let active_count = self.range_u64(2, 6) as usize;
        let start = self.range_u64(0, (self.players.len() - 1) as u64) as usize;

        let mut active_player_ids = Vec::with_capacity(active_count);
        for offset in 0..active_count {
            let idx = (start + offset) % self.players.len();
            active_player_ids.push(self.players[idx].id);
        }

        let mut player_map = serde_json::Map::new();
        let mut active_payload_by_id = HashMap::<&'static str, Value>::new();
        for (index, player_id) in active_player_ids.iter().enumerate() {
            let attacker = if active_player_ids.len() > 1 && self.chance(1, 4) {
                let mut idx =
                    self.range_u64(0, (active_player_ids.len() - 1) as u64)
                        as usize;
                if idx == index {
                    idx = (idx + 1) % active_player_ids.len();
                }
                self.identity_for(active_player_ids[idx]).name
            } else {
                ""
            };

            let (identity_name, identity_id, identity_primary_id, identity_shortcut, identity_team) = {
                let identity = self.identity_for(player_id);
                (
                    identity.name,
                    identity.id,
                    identity.primary_id,
                    identity.shortcut,
                    identity.team,
                )
            };

            let payload = json!({
                "name": identity_name,
                "id": identity_id,
                "primaryID": identity_primary_id,
                "team": identity_team,
                "score": self.range_u64(0, 900),
                "goals": self.range_u64(0, 5),
                "shots": self.range_u64(0, 8),
                "assists": self.range_u64(0, 4),
                "saves": self.range_u64(0, 6),
                "touches": self.range_u64(0, 60),
                "cartouches": self.range_u64(0, 45),
                "demos": self.range_u64(0, 4),
                "boost": self.range_u64(0, 100),
                "speed": self.range_u64(600, 2300),
                "hasCar": true,
                "isSonic": self.chance(1, 5),
                "isPowersliding": self.chance(1, 4),
                "isDead": self.chance(1, 20),
                "attacker": attacker,
                "shortcut": identity_shortcut,
                "onGround": self.chance(3, 4),
                "onWall": self.chance(1, 3),
                "location": {
                    "X": self.range_i64(-4096, 4096) as f64,
                    "Y": self.range_i64(-5120, 5120) as f64,
                    "Z": self.range_i64(17, 2044) as f64,
                    "pitch": self.range_i64(-314, 314) as f64 / 100.0,
                    "roll": self.range_i64(-314, 314) as f64 / 100.0,
                    "yaw": self.range_i64(-314, 314) as f64 / 100.0,
                }
            });

            self.seen_player_ids.insert(identity_id);
            self.last_player_payload_by_id
                .insert(identity_id, payload.clone());
            active_payload_by_id.insert(identity_id, payload);
        }

        for player in &self.players {
            let player_id = player.id;
            if !self.seen_player_ids.contains(player_id) {
                continue;
            }

            if let Some(payload) = active_payload_by_id
                .get(player_id)
                .cloned()
                .or_else(|| {
                    self.last_player_payload_by_id.get(player_id).cloned()
                })
            {
                player_map.insert(player_id.to_string(), payload);
            }
        }

        let has_winner = self.remaining_seconds == 0 && self.blue_score != self.orange_score;
        let winner = if has_winner {
            if self.blue_score > self.orange_score {
                "Blue"
            } else {
                "Orange"
            }
        } else {
            ""
        };

        json!({
            "match_guid": self.match_guid,
            "hasGame": true,
            "game": {
                "arena": "DFH Stadium",
                "time": self.remaining_seconds,
                "time_seconds": self.remaining_seconds,
                "isOT": self.remaining_seconds == 0 && self.blue_score == self.orange_score,
                "isReplay": self.chance(1, 10),
                "hasWinner": has_winner,
                "winner": winner,
                "hasTarget": true,
                "target": "Ball",
                "ball": {
                    "location": {
                        "X": self.range_i64(-4096, 4096) as f64,
                        "Y": self.range_i64(-5120, 5120) as f64,
                        "Z": self.range_i64(94, 2044) as f64,
                        "pitch": self.range_i64(-314, 314) as f64 / 100.0,
                        "roll": self.range_i64(-314, 314) as f64 / 100.0,
                        "yaw": self.range_i64(-314, 314) as f64 / 100.0,
                    },
                    "speed": self.range_u64(200, 2400),
                    "team": self.range_u64(0, 1),
                },
                "teams": [
                    {
                        "name": "Blue",
                        "score": self.blue_score,
                        "color_primary": "1873FF",
                        "color_secondary": "E5E5E5"
                    },
                    {
                        "name": "Orange",
                        "score": self.orange_score,
                        "color_primary": "C26418",
                        "color_secondary": "E5E5E5"
                    }
                ]
            },
            "players": Value::Object(player_map)
        })
    }
}

fn match_guid_for_scenario(scenario: &[SosEnvelope]) -> String {
    for envelope in scenario {
        if envelope.event == "game:update_state"
            && let Some(guid) = envelope
                .data
                .get("match_guid")
                .and_then(Value::as_str)
        {
            return guid.to_string();
        }
    }

    "M-EXAMPLE-123".to_string()
}

fn envelope_for_send(
    template: &SosEnvelope,
    update_state_randomizer: &mut UpdateStateRandomizer,
) -> SosEnvelope {
    if template.event == "game:update_state" {
        let mut randomized = template.clone();
        randomized.data = update_state_randomizer.next_update_state_payload();
        randomized
    } else {
        template.clone()
    }
}

fn debug_log(cli: &CliOptions, message: impl AsRef<str>) {
    if cli.debug {
        eprintln!("[debug] {}", message.as_ref());
    }
}

fn debug_event_log(
    cli: &CliOptions,
    action: &str,
    sos_event: &SosEnvelope,
    subscribers: Option<usize>,
    reason: Option<&str>,
) {
    if !cli.debug {
        return;
    }

    let log = json!({
        "type": "event",
        "action": action,
        "sos_event": sos_event,
        "subscribers": subscribers,
        "reason": reason,
    });

    eprintln!("{}", log);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let templates = load_template_envelopes()?;
    let parse_outcome = parse_args_from(std::env::args().skip(1))?;

    match parse_outcome {
        ParseOutcome::Help => {
            print_help();
            return Ok(());
        }
        ParseOutcome::ListEvents => {
            for event in available_events(&templates) {
                println!("{event}");
            }
            return Ok(());
        }
        ParseOutcome::Run(cli) => {
            run(cli, templates).await?;
        }
    }

    Ok(())
}

async fn run(
    cli: CliOptions,
    templates: Vec<SosEnvelope>,
) -> Result<(), Box<dyn std::error::Error>> {
    let scenario = select_scenario(&templates, &cli.event_filters)?;
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0xA5A5_5A5A_F00D_BAAD)
        ^ scenario.len() as u64;
    let mut update_state_randomizer =
        UpdateStateRandomizer::new(seed, match_guid_for_scenario(&scenario));

    let bind_address = format!("{}:{}", cli.ws_host, cli.ws_port);
    let listener = TcpListener::bind(&bind_address).await?;

    println!(
        "Mock broadcast websocket=ws://{}:{} interval_ms={} events={}",
        cli.ws_host,
        cli.ws_port,
        cli.interval_ms,
        scenario.len()
    );

    if cli.debug {
        let event_names = scenario
            .iter()
            .map(|event| event.event.clone())
            .collect::<Vec<_>>()
            .join(", ");
        debug_log(
            &cli,
            format!("scenario event order: [{event_names}]"),
        );
    }

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

    let shutdown_signal = wait_for_shutdown_signal();
    tokio::pin!(shutdown_signal);

    let mut sent = 0usize;
    let mut index = 0usize;

    let mut ticker = tokio::time::interval(Duration::from_millis(cli.interval_ms));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            signal_name = &mut shutdown_signal => {
                eprintln!("Received {signal_name}; shutting down mock broadcaster...");
                let _ = shutdown_tx.send(true);
                break;
            }
            _ = ticker.tick() => {
                let envelope = envelope_for_send(
                    &scenario[index],
                    &mut update_state_randomizer,
                );
                let payload = serde_json::to_string(&envelope)?;

                match outgoing_tx.send(payload) {
                    Ok(subscribers) => {
                        debug_event_log(
                            &cli,
                            "broadcast",
                            &envelope,
                            Some(subscribers),
                            None,
                        );
                    }
                    Err(_) => {
                        debug_event_log(
                            &cli,
                            "dropped",
                            &envelope,
                            None,
                            Some("no_websocket_clients"),
                        );
                    }
                }

                sent += 1;
                index += 1;

                if cli.max_events.is_some_and(|max| sent >= max) {
                    debug_log(
                        &cli,
                        format!("max-events reached ({}), stopping", sent),
                    );
                    let _ = shutdown_tx.send(true);
                    break;
                }

                if index >= scenario.len() {
                    if cli.loop_events {
                        index = 0;
                    } else {
                        debug_log(&cli, "scenario completed once; exiting (--no-loop)");
                        let _ = shutdown_tx.send(true);
                        break;
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
                        sleep(Duration::from_millis(500)).await;
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

fn load_template_envelopes(
) -> Result<Vec<SosEnvelope>, Box<dyn std::error::Error>> {
    let parsed: Vec<TemplateEnvelope> =
        serde_json::from_str(MOCK_ENVELOPES_JSON)?;

    Ok(parsed
        .into_iter()
        .map(|entry| SosEnvelope::new(entry.event, entry.data))
        .collect())
}

fn available_events(templates: &[SosEnvelope]) -> Vec<String> {
    let mut events = BTreeSet::new();
    for template in templates {
        events.insert(template.event.clone());
    }
    events.into_iter().collect()
}

fn select_scenario(
    templates: &[SosEnvelope],
    filters: &[String],
) -> Result<Vec<SosEnvelope>, Box<dyn std::error::Error>> {
    if filters.is_empty() {
        return Ok(templates.to_vec());
    }

    let wanted: HashSet<String> = filters.iter().cloned().collect();
    let known: HashSet<String> =
        templates.iter().map(|event| event.event.clone()).collect();

    let mut unknown = wanted
        .difference(&known)
        .cloned()
        .collect::<Vec<String>>();
    unknown.sort_unstable();

    if !unknown.is_empty() {
        let mut known_events = known.into_iter().collect::<Vec<String>>();
        known_events.sort_unstable();
        return Err(format!(
            "Unknown event filter(s): {}\nAvailable events: {}",
            unknown.join(", "),
            known_events.join(", "),
        )
        .into());
    }

    let selected = templates
        .iter()
        .filter(|event| wanted.contains(&event.event))
        .cloned()
        .collect::<Vec<_>>();

    if selected.is_empty() {
        return Err("event filter resulted in an empty scenario".into());
    }

    Ok(selected)
}

fn parse_args_from<I>(
    args: I,
) -> Result<ParseOutcome, Box<dyn std::error::Error>>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();

    let mut ws_host = "0.0.0.0".to_string();
    let mut ws_port = 49122u16;
    let mut interval_ms = 1000u64;
    let mut max_events = None;
    let mut event_filters = Vec::new();
    let mut loop_events = true;
    let mut debug = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--ws-host" => {
                let value = args.next().ok_or("--ws-host requires a value")?;
                ws_host = value;
            }
            "--ws-port" => {
                let value = args.next().ok_or("--ws-port requires a value")?;
                ws_port = value.parse::<u16>()?;
            }
            "--interval-ms" => {
                let value =
                    args.next().ok_or("--interval-ms requires a value")?;
                interval_ms = value.parse::<u64>()?;
                if interval_ms == 0 {
                    return Err("--interval-ms must be at least 1".into());
                }
            }
            "--max-events" => {
                let value =
                    args.next().ok_or("--max-events requires a value")?;
                max_events = Some(value.parse::<usize>()?);
            }
            "--event" | "--events" => {
                let value = args.next().ok_or("--event requires a value")?;
                parse_event_filter_value(&value, &mut event_filters);
            }
            "--no-loop" => {
                loop_events = false;
            }
            "--loop" => {
                loop_events = true;
            }
            "--list-events" => {
                return Ok(ParseOutcome::ListEvents);
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
        ws_host,
        ws_port,
        interval_ms,
        max_events,
        event_filters,
        loop_events,
        debug,
    }))
}

fn parse_event_filter_value(value: &str, out: &mut Vec<String>) {
    for raw in value.split(',') {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            out.push(trimmed.to_string());
        }
    }
}

fn print_help() {
    println!(
        "Usage: mock_broadcast [options]\n\nServe mock SOS websocket events to local/network overlay clients for UI and event-pipeline testing.\n\nWebsocket server:\n  --ws-host <host>       Bind host for websocket server (default 0.0.0.0)\n  --ws-port <port>       Bind port for websocket server (default 49122)\n\nMock event stream:\n  --event <name>         Filter to specific event(s); repeatable or comma-separated\n  --list-events          Print available event names and exit\n  --interval-ms <ms>     Delay between each sent event (default 1000, min 1)\n  --no-loop              Send selected scenario once then exit\n  --loop                 Keep looping scenario (default)\n  --max-events <n>       Stop after sending n total events\n\nNotes:\n  game:update_state payloads are randomized each send\n  game:update_state uses a fixed identity pool of at most 6 unique players\n  once a player appears in update_state they remain in later update_state payloads\n\nRuntime:\n  -d, --debug            Enable verbose debug logs\n  -h, --help             Show this help\n\nExamples:\n  cargo run --bin mock_broadcast --features dev-tools\n  cargo run --bin mock_broadcast --features dev-tools -- --event game:update_state --interval-ms 100\n  cargo run --bin mock_broadcast --features dev-tools -- --event game:goal_scored,game:replay_start --no-loop\n\nOverlay clients can connect to ws://localhost:49122"
    );
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use serde_json::Value;

    use super::{
        ParseOutcome, UpdateStateRandomizer, envelope_for_send,
        load_template_envelopes, match_guid_for_scenario, parse_args_from,
        select_scenario,
    };

    #[test]
    fn parse_args_uses_expected_defaults() {
        let parsed =
            parse_args_from(Vec::<String>::new()).expect("parse defaults");

        let ParseOutcome::Run(cli) = parsed else {
            panic!("expected runnable options");
        };

        assert_eq!(cli.ws_host, "0.0.0.0");
        assert_eq!(cli.ws_port, 49122);
        assert_eq!(cli.interval_ms, 1000);
        assert_eq!(cli.max_events, None);
        assert_eq!(cli.event_filters, Vec::<String>::new());
        assert!(cli.loop_events);
        assert!(!cli.debug);
    }

    #[test]
    fn parse_args_accepts_event_filters_and_overrides() {
        let parsed = parse_args_from(vec![
            "--ws-host".to_string(),
            "127.0.0.1".to_string(),
            "--ws-port".to_string(),
            "9001".to_string(),
            "--interval-ms".to_string(),
            "250".to_string(),
            "--max-events".to_string(),
            "12".to_string(),
            "--event".to_string(),
            "game:update_state,game:goal_scored".to_string(),
            "--event".to_string(),
            "game:replay_start".to_string(),
            "--no-loop".to_string(),
            "--debug".to_string(),
        ])
        .expect("parse override args");

        let ParseOutcome::Run(cli) = parsed else {
            panic!("expected runnable options");
        };

        assert_eq!(cli.ws_host, "127.0.0.1");
        assert_eq!(cli.ws_port, 9001);
        assert_eq!(cli.interval_ms, 250);
        assert_eq!(cli.max_events, Some(12));
        assert_eq!(
            cli.event_filters,
            vec![
                "game:update_state".to_string(),
                "game:goal_scored".to_string(),
                "game:replay_start".to_string()
            ]
        );
        assert!(!cli.loop_events);
        assert!(cli.debug);
    }

    #[test]
    fn parse_args_recognizes_help_and_list() {
        let help =
            parse_args_from(vec!["--help".to_string()]).expect("parse help");
        assert!(matches!(help, ParseOutcome::Help));

        let list = parse_args_from(vec!["--list-events".to_string()])
            .expect("parse list-events");
        assert!(matches!(list, ParseOutcome::ListEvents));
    }

    #[test]
    fn select_scenario_filters_to_requested_events() {
        let templates = load_template_envelopes().expect("load templates");
        let filtered = select_scenario(
            &templates,
            &["game:goal_scored".to_string(), "game:replay_start".to_string()],
        )
        .expect("select scenario");

        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].event, "game:goal_scored");
        assert_eq!(filtered[1].event, "game:replay_start");
    }

    #[test]
    fn select_scenario_rejects_unknown_events() {
        let templates = load_template_envelopes().expect("load templates");
        let error =
            select_scenario(&templates, &["game:not_real".to_string()])
                .expect_err("unknown event should fail");

        let message = error.to_string();
        assert!(message.contains("Unknown event filter"));
        assert!(message.contains("game:not_real"));
    }

    #[test]
    fn randomized_update_state_changes_between_sends() {
        let templates = load_template_envelopes().expect("load templates");
        let update_state = templates
            .iter()
            .find(|value| value.event == "game:update_state")
            .expect("has update_state template");
        let mut randomizer =
            UpdateStateRandomizer::new(42, match_guid_for_scenario(&templates));

        let first = envelope_for_send(update_state, &mut randomizer);
        let second = envelope_for_send(update_state, &mut randomizer);

        assert_ne!(first.data, second.data);
    }

    #[test]
    fn randomized_update_state_caps_unique_player_ids_to_six() {
        let templates = load_template_envelopes().expect("load templates");
        let update_state = templates
            .iter()
            .find(|value| value.event == "game:update_state")
            .expect("has update_state template");
        let mut randomizer =
            UpdateStateRandomizer::new(7, match_guid_for_scenario(&templates));

        let mut unique_ids = HashSet::new();

        for _ in 0..50 {
            let envelope = envelope_for_send(update_state, &mut randomizer);
            let players = envelope
                .data
                .get("players")
                .and_then(Value::as_object)
                .expect("update_state players object");

            for key in players.keys() {
                unique_ids.insert(key.clone());
            }
        }

        assert!(unique_ids.len() <= 6);
    }

    #[test]
    fn randomized_update_state_keeps_player_name_and_team_stable_per_id() {
        let templates = load_template_envelopes().expect("load templates");
        let update_state = templates
            .iter()
            .find(|value| value.event == "game:update_state")
            .expect("has update_state template");
        let mut randomizer =
            UpdateStateRandomizer::new(99, match_guid_for_scenario(&templates));

        let mut observed_identity = HashMap::<String, (String, u64)>::new();

        for _ in 0..60 {
            let envelope = envelope_for_send(update_state, &mut randomizer);
            let players = envelope
                .data
                .get("players")
                .and_then(Value::as_object)
                .expect("update_state players object");

            for (id, player) in players {
                let name = player
                    .get("name")
                    .and_then(Value::as_str)
                    .expect("player name")
                    .to_string();
                let team = player
                    .get("team")
                    .and_then(Value::as_u64)
                    .expect("player team");

                if let Some((known_name, known_team)) = observed_identity.get(id)
                {
                    assert_eq!(&name, known_name);
                    assert_eq!(&team, known_team);
                } else {
                    observed_identity.insert(id.clone(), (name, team));
                }
            }
        }

        assert!(observed_identity.len() <= 6);
    }

    #[test]
    fn randomized_update_state_never_drops_seen_players() {
        let templates = load_template_envelopes().expect("load templates");
        let update_state = templates
            .iter()
            .find(|value| value.event == "game:update_state")
            .expect("has update_state template");
        let mut randomizer =
            UpdateStateRandomizer::new(123, match_guid_for_scenario(&templates));

        let mut previous_ids = HashSet::<String>::new();

        for _ in 0..60 {
            let envelope = envelope_for_send(update_state, &mut randomizer);
            let players = envelope
                .data
                .get("players")
                .and_then(Value::as_object)
                .expect("update_state players object");

            let current_ids = players.keys().cloned().collect::<HashSet<_>>();
            assert!(current_ids.is_superset(&previous_ids));
            previous_ids = current_ids;
        }

        assert!(previous_ids.len() <= 6);
    }
}