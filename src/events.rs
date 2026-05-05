use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::RlStatsError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope<T = Value> {
    #[serde(rename = "Event", alias = "event")]
    pub event: String,
    #[serde(rename = "Data", alias = "data")]
    pub data: T,
}

#[derive(Debug, Clone)]
pub enum StatsEvent {
    UpdateState(UpdateStateData),
    BallHit(BallHitData),
    ClockUpdatedSeconds(ClockUpdatedSecondsData),
    CountdownBegin(MatchOnlyData),
    CrossbarHit(CrossbarHitData),
    GoalReplayEnd(MatchOnlyData),
    GoalReplayStart(MatchOnlyData),
    GoalReplayWillEnd(MatchOnlyData),
    GoalScored(GoalScoredData),
    MatchCreated(MatchOnlyData),
    MatchInitialized(MatchOnlyData),
    MatchDestroyed(MatchOnlyData),
    MatchEnded(MatchEndedData),
    MatchPaused(MatchOnlyData),
    MatchUnpaused(MatchOnlyData),
    PodiumStart(MatchOnlyData),
    ReplayCreated(MatchOnlyData),
    RoundStarted(MatchOnlyData),
    StatfeedEvent(StatfeedEventData),
    Unknown(UnknownEvent),
}

#[derive(Debug, Clone)]
pub struct UnknownEvent {
    pub event: String,
    pub data: Value,
}

pub fn stats_event_name(event: &StatsEvent) -> &'static str {
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

pub fn is_bogus_goal_scored(data: &GoalScoredData) -> bool {
    let speed_is_zero = data.goal_speed.abs() <= f64::EPSILON;
    let time_is_zero = data.goal_time.abs() <= f64::EPSILON;
    let scorer_missing = data.scorer.name.trim().is_empty() && data.scorer.shortcut == 0;
    let assister_missing = data.assister.as_ref().is_none_or(|assister| {
        assister.name.trim().is_empty() && assister.shortcut == 0
    });

    speed_is_zero && time_is_zero && scorer_missing && assister_missing
}

pub fn stats_event_to_value(event: &StatsEvent) -> Result<Value, RlStatsError> {
    let value = match event {
        StatsEvent::UpdateState(data) => {
            json!({"event": "UpdateState", "data": data})
        }
        StatsEvent::BallHit(data) => {
            json!({"event": "BallHit", "data": data})
        }
        StatsEvent::ClockUpdatedSeconds(data) => {
            json!({"event": "ClockUpdatedSeconds", "data": data})
        }
        StatsEvent::CountdownBegin(data) => {
            json!({"event": "CountdownBegin", "data": data})
        }
        StatsEvent::CrossbarHit(data) => {
            json!({"event": "CrossbarHit", "data": data})
        }
        StatsEvent::GoalReplayEnd(data) => {
            json!({"event": "GoalReplayEnd", "data": data})
        }
        StatsEvent::GoalReplayStart(data) => {
            json!({"event": "GoalReplayStart", "data": data})
        }
        StatsEvent::GoalReplayWillEnd(data) => {
            json!({"event": "GoalReplayWillEnd", "data": data})
        }
        StatsEvent::GoalScored(data) => {
            json!({"event": "GoalScored", "data": data})
        }
        StatsEvent::MatchCreated(data) => {
            json!({"event": "MatchCreated", "data": data})
        }
        StatsEvent::MatchInitialized(data) => {
            json!({"event": "MatchInitialized", "data": data})
        }
        StatsEvent::MatchDestroyed(data) => {
            json!({"event": "MatchDestroyed", "data": data})
        }
        StatsEvent::MatchEnded(data) => {
            json!({"event": "MatchEnded", "data": data})
        }
        StatsEvent::MatchPaused(data) => {
            json!({"event": "MatchPaused", "data": data})
        }
        StatsEvent::MatchUnpaused(data) => {
            json!({"event": "MatchUnpaused", "data": data})
        }
        StatsEvent::PodiumStart(data) => {
            json!({"event": "PodiumStart", "data": data})
        }
        StatsEvent::ReplayCreated(data) => {
            json!({"event": "ReplayCreated", "data": data})
        }
        StatsEvent::RoundStarted(data) => {
            json!({"event": "RoundStarted", "data": data})
        }
        StatsEvent::StatfeedEvent(data) => {
            json!({"event": "StatfeedEvent", "data": data})
        }
        StatsEvent::Unknown(data) => json!({
            "event": data.event,
            "data": data.data,
        }),
    };

    Ok(value)
}

pub fn parse_stats_event(input: &str) -> Result<StatsEvent, RlStatsError> {
    let envelope: EventEnvelope<Value> = serde_json::from_str(input)?;
    parse_event_envelope(envelope)
}

pub fn parse_stats_event_value(
    value: Value,
) -> Result<StatsEvent, RlStatsError> {
    let envelope: EventEnvelope<Value> = serde_json::from_value(value)?;
    parse_event_envelope(envelope)
}

fn parse_event_envelope(
    envelope: EventEnvelope<Value>,
) -> Result<StatsEvent, RlStatsError> {
    let data = normalize_event_data(envelope.data)?;

    let event = match envelope.event.as_str() {
        "UpdateState" => StatsEvent::UpdateState(serde_json::from_value(data)?),
        "BallHit" => StatsEvent::BallHit(serde_json::from_value(data)?),
        "ClockUpdatedSeconds" => {
            StatsEvent::ClockUpdatedSeconds(serde_json::from_value(data)?)
        }
        "CountdownBegin" => {
            StatsEvent::CountdownBegin(serde_json::from_value(data)?)
        }
        "CrossbarHit" => StatsEvent::CrossbarHit(serde_json::from_value(data)?),
        "GoalReplayEnd" | "ReplayEnd" => {
            StatsEvent::GoalReplayEnd(serde_json::from_value(data)?)
        }
        "GoalReplayStart" | "ReplayStart" => {
            StatsEvent::GoalReplayStart(serde_json::from_value(data)?)
        }
        "GoalReplayWillEnd" | "ReplayWillEnd" => {
            StatsEvent::GoalReplayWillEnd(serde_json::from_value(data)?)
        }
        "GoalScored" => StatsEvent::GoalScored(serde_json::from_value(data)?),
        "MatchCreated" => {
            StatsEvent::MatchCreated(serde_json::from_value(data)?)
        }
        "MatchInitialized" => {
            StatsEvent::MatchInitialized(serde_json::from_value(data)?)
        }
        "MatchDestroyed" => {
            StatsEvent::MatchDestroyed(serde_json::from_value(data)?)
        }
        "MatchEnded" => StatsEvent::MatchEnded(serde_json::from_value(data)?),
        "MatchPaused" => StatsEvent::MatchPaused(serde_json::from_value(data)?),
        "MatchUnpaused" => {
            StatsEvent::MatchUnpaused(serde_json::from_value(data)?)
        }
        "PodiumStart" => StatsEvent::PodiumStart(serde_json::from_value(data)?),
        "ReplayCreated" => {
            StatsEvent::ReplayCreated(serde_json::from_value(data)?)
        }
        "RoundStarted" => {
            StatsEvent::RoundStarted(serde_json::from_value(data)?)
        }
        "StatfeedEvent" => {
            StatsEvent::StatfeedEvent(serde_json::from_value(data)?)
        }
        _ => StatsEvent::Unknown(UnknownEvent {
            event: envelope.event,
            data,
        }),
    };

    Ok(event)
}

fn normalize_event_data(data: Value) -> Result<Value, RlStatsError> {
    match data {
        Value::String(raw) => {
            let parsed = serde_json::from_str::<Value>(&raw)?;
            Ok(parsed)
        }
        other => Ok(other),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MatchOnlyData {
    #[serde(rename = "MatchGuid", default)]
    pub match_guid: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Vector3 {
    #[serde(rename = "X", default)]
    pub x: f64,
    #[serde(rename = "Y", default)]
    pub y: f64,
    #[serde(rename = "Z", default)]
    pub z: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayerRef {
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "Shortcut", default)]
    pub shortcut: i64,
    #[serde(rename = "TeamNum", default)]
    pub team_num: i64,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LastTouch {
    #[serde(rename = "Player", default)]
    pub player: PlayerRef,
    #[serde(rename = "Speed", default)]
    pub speed: f64,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BallHitBall {
    #[serde(rename = "PreHitSpeed", default)]
    pub pre_hit_speed: f64,
    #[serde(rename = "PostHitSpeed", default)]
    pub post_hit_speed: f64,
    #[serde(rename = "Location")]
    pub location: Vector3,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BallHitData {
    #[serde(rename = "MatchGuid", default)]
    pub match_guid: Option<String>,
    #[serde(rename = "Players", default)]
    pub players: Vec<PlayerRef>,
    #[serde(rename = "Ball", default)]
    pub ball: BallHitBall,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClockUpdatedSecondsData {
    #[serde(rename = "MatchGuid", default)]
    pub match_guid: Option<String>,
    #[serde(rename = "TimeSeconds", default)]
    pub time_seconds: i64,
    #[serde(rename = "bOvertime", default)]
    pub b_overtime: bool,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CrossbarHitData {
    #[serde(rename = "MatchGuid", default)]
    pub match_guid: Option<String>,
    #[serde(rename = "BallLocation")]
    pub ball_location: Vector3,
    #[serde(rename = "BallSpeed", default)]
    pub ball_speed: f64,
    #[serde(rename = "ImpactForce", default)]
    pub impact_force: f64,
    #[serde(rename = "BallLastTouch", default)]
    pub ball_last_touch: LastTouch,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GoalScoredData {
    #[serde(rename = "MatchGuid", default)]
    pub match_guid: Option<String>,
    #[serde(rename = "GoalSpeed", default)]
    pub goal_speed: f64,
    #[serde(rename = "GoalTime", default)]
    pub goal_time: f64,
    #[serde(rename = "ImpactLocation")]
    pub impact_location: Vector3,
    #[serde(rename = "Scorer", default)]
    pub scorer: PlayerRef,
    #[serde(rename = "Assister", default)]
    pub assister: Option<PlayerRef>,
    #[serde(rename = "BallLastTouch", default)]
    pub ball_last_touch: LastTouch,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MatchEndedData {
    #[serde(rename = "MatchGuid", default)]
    pub match_guid: Option<String>,
    #[serde(rename = "WinnerTeamNum", default)]
    pub winner_team_num: i64,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatfeedEventData {
    #[serde(rename = "MatchGuid", default)]
    pub match_guid: Option<String>,
    #[serde(rename = "EventName", default)]
    pub event_name: String,
    #[serde(rename = "Type", default)]
    pub type_label: String,
    #[serde(rename = "MainTarget", default)]
    pub main_target: PlayerRef,
    #[serde(rename = "SecondaryTarget", default)]
    pub secondary_target: Option<PlayerRef>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TeamState {
    #[serde(rename = "Name", default)]
    pub name: Option<String>,
    #[serde(rename = "TeamNum", default)]
    pub team_num: Option<i64>,
    #[serde(rename = "Score", default)]
    pub score: Option<i64>,
    #[serde(rename = "ColorPrimary", default)]
    pub color_primary: Option<String>,
    #[serde(rename = "ColorSecondary", default)]
    pub color_secondary: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BallState {
    #[serde(rename = "Speed", default)]
    pub speed: Option<f64>,
    #[serde(rename = "TeamNum", default)]
    pub team_num: Option<i64>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateStateGame {
    #[serde(rename = "Teams", default)]
    pub teams: Vec<TeamState>,
    #[serde(rename = "TimeSeconds", default)]
    pub time_seconds: Option<i64>,
    #[serde(rename = "bOvertime", default)]
    pub b_overtime: Option<bool>,
    #[serde(rename = "Frame", default)]
    pub frame: Option<i64>,
    #[serde(rename = "Elapsed", default)]
    pub elapsed: Option<f64>,
    #[serde(rename = "Ball", default)]
    pub ball: Option<BallState>,
    #[serde(rename = "bReplay", default)]
    pub b_replay: Option<bool>,
    #[serde(rename = "bHasWinner", default)]
    pub b_has_winner: Option<bool>,
    #[serde(rename = "Winner", default)]
    pub winner: Option<String>,
    #[serde(rename = "Arena", default)]
    pub arena: Option<String>,
    #[serde(rename = "bHasTarget", default)]
    pub b_has_target: Option<bool>,
    #[serde(rename = "Target", default)]
    pub target: Option<PlayerRef>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateStatePlayer {
    #[serde(rename = "Name", default)]
    pub name: Option<String>,
    #[serde(rename = "PrimaryId", default)]
    pub primary_id: Option<String>,
    #[serde(rename = "Shortcut", default)]
    pub shortcut: Option<i64>,
    #[serde(rename = "TeamNum", default)]
    pub team_num: Option<i64>,
    #[serde(rename = "Score", default)]
    pub score: Option<i64>,
    #[serde(rename = "Goals", default)]
    pub goals: Option<i64>,
    #[serde(rename = "Shots", default)]
    pub shots: Option<i64>,
    #[serde(rename = "Assists", default)]
    pub assists: Option<i64>,
    #[serde(rename = "Saves", default)]
    pub saves: Option<i64>,
    #[serde(rename = "Touches", default)]
    pub touches: Option<i64>,
    #[serde(rename = "CarTouches", default)]
    pub car_touches: Option<i64>,
    #[serde(rename = "Demos", default)]
    pub demos: Option<i64>,
    #[serde(rename = "bHasCar", default)]
    pub b_has_car: Option<bool>,
    #[serde(rename = "Speed", default)]
    pub speed: Option<f64>,
    #[serde(rename = "Boost", default)]
    pub boost: Option<i64>,
    #[serde(rename = "bBoosting", default)]
    pub b_boosting: Option<bool>,
    #[serde(rename = "bOnGround", default)]
    pub b_on_ground: Option<bool>,
    #[serde(rename = "bOnWall", default)]
    pub b_on_wall: Option<bool>,
    #[serde(rename = "bPowersliding", default)]
    pub b_powersliding: Option<bool>,
    #[serde(rename = "bDemolished", default)]
    pub b_demolished: Option<bool>,
    #[serde(rename = "bSupersonic", default)]
    pub b_supersonic: Option<bool>,
    #[serde(rename = "Attacker", default)]
    pub attacker: Option<PlayerRef>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

impl UpdateStatePlayer {
    pub fn effective_speed(&self) -> Option<f64> {
        self.speed
            .or_else(|| {
                first_numeric_f64(
                    &self.extra,
                    &[
                        "speed",
                        "Speed",
                        "CarSpeed",
                        "carSpeed",
                        "car_speed",
                        "Velocity",
                        "velocity",
                        "Car.Speed",
                        "car.speed",
                    ],
                )
            })
            .or_else(|| {
                first_nested_numeric_f64(
                    &self.extra,
                    &[
                        "Car", "car", "CarData", "carData", "car_data",
                        "Vehicle", "vehicle",
                    ],
                    &[
                        "Speed",
                        "speed",
                        "CarSpeed",
                        "carSpeed",
                        "car_speed",
                        "Velocity",
                        "velocity",
                    ],
                )
            })
    }

    pub fn effective_boost(&self) -> Option<i64> {
        self.boost
            .or_else(|| {
                first_numeric_i64(
                    &self.extra,
                    &[
                        "boost",
                        "Boost",
                        "BoostAmount",
                        "boostAmount",
                        "boost_amount",
                        "Car.Boost",
                        "car.boost",
                    ],
                )
            })
            .or_else(|| {
                first_nested_numeric_i64(
                    &self.extra,
                    &[
                        "Car", "car", "CarData", "carData", "car_data",
                        "Vehicle", "vehicle",
                    ],
                    &[
                        "Boost",
                        "boost",
                        "BoostAmount",
                        "boostAmount",
                        "boost_amount",
                    ],
                )
            })
    }

    pub fn effective_boosting(&self) -> Option<bool> {
        self.b_boosting
            .or_else(|| {
                first_bool(
                    &self.extra,
                    &["bBoosting", "boosting", "isBoosting"],
                )
            })
            .or_else(|| {
                first_nested_bool(
                    &self.extra,
                    &[
                        "Car", "car", "CarData", "carData", "car_data",
                        "Vehicle", "vehicle",
                    ],
                    &["bBoosting", "boosting", "isBoosting"],
                )
            })
    }

    pub fn effective_supersonic(&self) -> Option<bool> {
        self.b_supersonic
            .or_else(|| {
                first_bool(
                    &self.extra,
                    &["bSupersonic", "supersonic", "isSupersonic"],
                )
            })
            .or_else(|| {
                first_nested_bool(
                    &self.extra,
                    &[
                        "Car", "car", "CarData", "carData", "car_data",
                        "Vehicle", "vehicle",
                    ],
                    &["bSupersonic", "supersonic", "isSupersonic"],
                )
            })
    }
}

fn first_numeric_f64(
    map: &HashMap<String, Value>,
    keys: &[&str],
) -> Option<f64> {
    keys.iter()
        .find_map(|key| map.get(*key).and_then(value_as_f64))
}

fn first_numeric_i64(
    map: &HashMap<String, Value>,
    keys: &[&str],
) -> Option<i64> {
    keys.iter()
        .find_map(|key| map.get(*key).and_then(value_as_i64))
}

fn first_nested_numeric_f64(
    map: &HashMap<String, Value>,
    container_keys: &[&str],
    value_keys: &[&str],
) -> Option<f64> {
    container_keys.iter().find_map(|container_key| {
        let Value::Object(object) = map.get(*container_key)? else {
            return None;
        };

        value_keys
            .iter()
            .find_map(|value_key| object.get(*value_key).and_then(value_as_f64))
    })
}

fn first_nested_numeric_i64(
    map: &HashMap<String, Value>,
    container_keys: &[&str],
    value_keys: &[&str],
) -> Option<i64> {
    container_keys.iter().find_map(|container_key| {
        let Value::Object(object) = map.get(*container_key)? else {
            return None;
        };

        value_keys
            .iter()
            .find_map(|value_key| object.get(*value_key).and_then(value_as_i64))
    })
}

fn first_bool(map: &HashMap<String, Value>, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| map.get(*key).and_then(value_as_bool))
}

fn first_nested_bool(
    map: &HashMap<String, Value>,
    container_keys: &[&str],
    value_keys: &[&str],
) -> Option<bool> {
    container_keys.iter().find_map(|container_key| {
        let Value::Object(object) = map.get(*container_key)? else {
            return None;
        };

        value_keys.iter().find_map(|value_key| {
            object.get(*value_key).and_then(value_as_bool)
        })
    })
}

fn value_as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number
            .as_f64()
            .or_else(|| number.as_i64().map(|v| v as f64)),
        Value::String(text) => text.parse::<f64>().ok(),
        _ => None,
    }
}

fn value_as_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|v| v.trunc() as i64)),
        Value::String(text) => text
            .parse::<i64>()
            .ok()
            .or_else(|| text.parse::<f64>().ok().map(|v| v.trunc() as i64)),
        _ => None,
    }
}

fn value_as_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(boolean) => Some(*boolean),
        Value::Number(number) => number.as_i64().map(|v| v != 0),
        Value::String(text) => {
            let normalized = text.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "true" | "1" | "yes" | "y" | "on" => Some(true),
                "false" | "0" | "no" | "n" | "off" => Some(false),
                _ => None,
            }
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateStateData {
    #[serde(rename = "MatchGuid", default)]
    pub match_guid: Option<String>,
    #[serde(rename = "Players", default)]
    pub players: Vec<UpdateStatePlayer>,
    #[serde(rename = "Game", default)]
    pub game: UpdateStateGame,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}
