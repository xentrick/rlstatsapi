use std::path::PathBuf;

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyModule;
use serde_json::json;
use tokio::runtime::Runtime;

use crate::{
    ClientOptions, EventFilter, EventKind, MatchSignal,
    RocketLeagueStatsClient, parse_stats_event, stats_event_name,
    stats_event_to_value, to_match_signal, winner_team_num,
};

#[pyclass(name = "RocketLeagueStatsClient", unsendable)]
pub struct PyRocketLeagueStatsClient {
    options: ClientOptions,
    runtime: Runtime,
    client: Option<RocketLeagueStatsClient>,
}

#[pymethods]
impl PyRocketLeagueStatsClient {
    #[new]
    #[pyo3(signature = (
        host = "127.0.0.1".to_string(),
        port = 49123,
        ini_path = None,
        auto_enable_packet_rate = true,
        packet_send_rate = 60.0,
        set_packet_rate_only_when_zero = true
    ))]
    fn new(
        host: String,
        port: u16,
        ini_path: Option<String>,
        auto_enable_packet_rate: bool,
        packet_send_rate: f32,
        set_packet_rate_only_when_zero: bool,
    ) -> PyResult<Self> {
        let mut options = ClientOptions::default();
        options.host = host;
        options.port_override = Some(port);
        options.auto_enable_packet_rate = auto_enable_packet_rate;
        options.packet_send_rate = packet_send_rate;
        options.set_packet_rate_only_when_zero = set_packet_rate_only_when_zero;
        options.stats_api_ini_path = ini_path.map(PathBuf::from);

        let runtime = Runtime::new().map_err(to_runtime_err)?;

        Ok(Self {
            options,
            runtime,
            client: None,
        })
    }

    fn connect(&mut self) -> PyResult<()> {
        let client = self
            .runtime
            .block_on(RocketLeagueStatsClient::connect(self.options.clone()))
            .map_err(to_runtime_err)?;

        self.client = Some(client);
        Ok(())
    }

    fn reconnect(&mut self) -> PyResult<()> {
        if self.client.is_none() {
            return self.connect();
        }

        let client = self
            .client
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("client not connected"))?;
        self.runtime
            .block_on(client.reconnect())
            .map_err(to_runtime_err)
    }

    fn next_event_json(&mut self) -> PyResult<Option<String>> {
        self.ensure_connected()?;

        let client = self
            .client
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("client not connected"))?;

        let event = self
            .runtime
            .block_on(client.next_event())
            .map_err(to_runtime_err)?;

        match event {
            Some(event) => {
                let value =
                    stats_event_to_value(&event).map_err(to_runtime_err)?;
                let serialized =
                    serde_json::to_string(&value).map_err(to_runtime_err)?;
                Ok(Some(serialized))
            }
            None => Ok(None),
        }
    }

    #[pyo3(signature = (
        event_types = None,
        player_name = None,
        player_primary_id = None,
        team_num = None,
        match_guid = None
    ))]
    fn next_filtered_event_json(
        &mut self,
        event_types: Option<Vec<String>>,
        player_name: Option<String>,
        player_primary_id: Option<String>,
        team_num: Option<i64>,
        match_guid: Option<String>,
    ) -> PyResult<Option<String>> {
        self.ensure_connected()?;

        let filter = build_filter(
            event_types,
            player_name,
            player_primary_id,
            team_num,
            match_guid,
        )?;

        let client = self
            .client
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("client not connected"))?;

        let event = self
            .runtime
            .block_on(client.next_filtered_event(&filter))
            .map_err(to_runtime_err)?;

        match event {
            Some(event) => {
                let value =
                    stats_event_to_value(&event).map_err(to_runtime_err)?;
                let serialized =
                    serde_json::to_string(&value).map_err(to_runtime_err)?;
                Ok(Some(serialized))
            }
            None => Ok(None),
        }
    }

    fn close(&mut self) -> PyResult<()> {
        if let Some(client) = self.client.take() {
            self.runtime
                .block_on(client.close())
                .map_err(to_runtime_err)?;
        }

        Ok(())
    }

    fn socket_address(&self) -> String {
        let host = &self.options.host;
        let port = self.options.port_override.unwrap_or(49123);
        format!("{host}:{port}")
    }
}

impl PyRocketLeagueStatsClient {
    fn ensure_connected(&mut self) -> PyResult<()> {
        if self.client.is_none() {
            self.connect()?;
        }

        Ok(())
    }
}

#[pyfunction]
fn parse_event_json(raw: &str) -> PyResult<String> {
    let event = parse_stats_event(raw).map_err(to_value_err)?;
    let value = stats_event_to_value(&event).map_err(to_value_err)?;
    serde_json::to_string(&value).map_err(to_value_err)
}

#[pyfunction]
fn event_name(raw: &str) -> PyResult<String> {
    let event = parse_stats_event(raw).map_err(to_value_err)?;
    Ok(stats_event_name(&event).to_string())
}

#[pyfunction]
fn list_event_kinds() -> Vec<String> {
    [
        "update_state",
        "ball_hit",
        "clock_updated_seconds",
        "countdown_begin",
        "crossbar_hit",
        "goal_replay_end",
        "goal_replay_start",
        "goal_replay_will_end",
        "goal_scored",
        "match_created",
        "match_initialized",
        "match_destroyed",
        "match_ended",
        "match_paused",
        "match_unpaused",
        "podium_start",
        "replay_created",
        "round_started",
        "statfeed_event",
        "unknown",
    ]
    .iter()
    .map(|value| (*value).to_string())
    .collect()
}

#[pyfunction]
#[pyo3(signature = (
    raw,
    event_types = None,
    player_name = None,
    player_primary_id = None,
    team_num = None,
    match_guid = None
))]
fn event_matches(
    raw: &str,
    event_types: Option<Vec<String>>,
    player_name: Option<String>,
    player_primary_id: Option<String>,
    team_num: Option<i64>,
    match_guid: Option<String>,
) -> PyResult<bool> {
    let event = parse_stats_event(raw).map_err(to_value_err)?;
    let filter = build_filter(
        event_types,
        player_name,
        player_primary_id,
        team_num,
        match_guid,
    )?;
    Ok(filter.matches(&event))
}

#[pyfunction]
#[pyo3(signature = (
    raw,
    event_types = None,
    player_name = None,
    player_primary_id = None,
    team_num = None,
    match_guid = None
))]
fn filter_event_json(
    raw: &str,
    event_types: Option<Vec<String>>,
    player_name: Option<String>,
    player_primary_id: Option<String>,
    team_num: Option<i64>,
    match_guid: Option<String>,
) -> PyResult<Option<String>> {
    let event = parse_stats_event(raw).map_err(to_value_err)?;
    let filter = build_filter(
        event_types,
        player_name,
        player_primary_id,
        team_num,
        match_guid,
    )?;

    if !filter.matches(&event) {
        return Ok(None);
    }

    let value = stats_event_to_value(&event).map_err(to_value_err)?;
    let serialized = serde_json::to_string(&value).map_err(to_value_err)?;
    Ok(Some(serialized))
}

#[pyfunction]
fn winner_team(raw: &str) -> PyResult<Option<i64>> {
    let event = parse_stats_event(raw).map_err(to_value_err)?;
    Ok(winner_team_num(&event))
}

#[pyfunction]
fn match_signal_json(raw: &str) -> PyResult<Option<String>> {
    let event = parse_stats_event(raw).map_err(to_value_err)?;

    let signal = match to_match_signal(&event) {
        Some(MatchSignal::GoalScored(data)) => {
            json!({"signal": "goal_scored", "data": data})
        }
        Some(MatchSignal::MatchConcluded(data)) => {
            json!({"signal": "match_concluded", "data": data})
        }
        None => return Ok(None),
    };

    let serialized = serde_json::to_string(&signal).map_err(to_value_err)?;
    Ok(Some(serialized))
}

#[pymodule]
fn rlstatsapi(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyRocketLeagueStatsClient>()?;
    m.add_function(wrap_pyfunction!(parse_event_json, m)?)?;
    m.add_function(wrap_pyfunction!(event_name, m)?)?;
    m.add_function(wrap_pyfunction!(list_event_kinds, m)?)?;
    m.add_function(wrap_pyfunction!(event_matches, m)?)?;
    m.add_function(wrap_pyfunction!(filter_event_json, m)?)?;
    m.add_function(wrap_pyfunction!(winner_team, m)?)?;
    m.add_function(wrap_pyfunction!(match_signal_json, m)?)?;
    Ok(())
}

fn build_filter(
    event_types: Option<Vec<String>>,
    player_name: Option<String>,
    player_primary_id: Option<String>,
    team_num: Option<i64>,
    match_guid: Option<String>,
) -> PyResult<EventFilter> {
    let mut filter = EventFilter::new();

    if let Some(tokens) = event_types {
        let mut kinds = Vec::new();
        for token in tokens {
            for part in token.split(',') {
                let normalized = part.trim().to_ascii_lowercase();
                if normalized.is_empty() || normalized == "all" {
                    continue;
                }

                let kind = parse_event_kind(&normalized).ok_or_else(|| {
                    PyValueError::new_err(format!(
                        "unknown event type '{normalized}'"
                    ))
                })?;
                kinds.push(kind);
            }
        }

        if !kinds.is_empty() {
            filter = filter.include_kinds(kinds);
        }
    }

    if let Some(name) = player_name {
        filter = filter.with_player_name(name);
    }
    if let Some(primary_id) = player_primary_id {
        filter = filter.with_player_primary_id(primary_id);
    }
    if let Some(team_num) = team_num {
        filter = filter.with_team_num(team_num);
    }
    if let Some(match_guid) = match_guid {
        filter = filter.with_match_guid(match_guid);
    }

    Ok(filter)
}

fn parse_event_kind(token: &str) -> Option<EventKind> {
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

fn to_runtime_err<E: std::fmt::Display>(error: E) -> PyErr {
    PyRuntimeError::new_err(error.to_string())
}

fn to_value_err<E: std::fmt::Display>(error: E) -> PyErr {
    PyValueError::new_err(error.to_string())
}
