use std::collections::HashMap;

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::error::RlStatsError;
use crate::events::{
    MatchOnlyData, PlayerRef, StatsEvent, TeamState, UpdateStateData,
    stats_event_to_value,
};

pub const SOS_VERSION: &str = "1.6.0-beta.6-rlstatsapi";

#[derive(Debug, Clone, Serialize)]
pub struct SosEnvelope {
    pub event: String,
    pub data: Value,
}

impl SosEnvelope {
    pub fn new(event: impl Into<String>, data: Value) -> Self {
        Self {
            event: event.into(),
            data,
        }
    }
}

pub fn sos_version_envelope() -> SosEnvelope {
    SosEnvelope::new("sos:version", Value::String(SOS_VERSION.to_string()))
}

pub fn translate_stats_event(
    event: &StatsEvent,
) -> Result<Vec<SosEnvelope>, RlStatsError> {
    let mapped = match event {
        StatsEvent::MatchCreated(data) => {
            vec![SosEnvelope::new(
                "game:match_created",
                match_guid_payload(data),
            )]
        }
        StatsEvent::MatchInitialized(data) => {
            vec![SosEnvelope::new(
                "game:initialized",
                match_guid_payload(data),
            )]
        }
        StatsEvent::CountdownBegin(data) => {
            let payload = match_guid_payload(data);
            vec![
                SosEnvelope::new("game:pre_countdown_begin", payload.clone()),
                SosEnvelope::new("game:post_countdown_begin", payload),
            ]
        }
        StatsEvent::UpdateState(data) => {
            vec![SosEnvelope::new(
                "game:update_state",
                translate_update_state(data),
            )]
        }
        StatsEvent::BallHit(data) => {
            let player = data.players.first();
            let player_name =
                player.map(|value| value.name.clone()).unwrap_or_default();
            let player_id = player.map(player_ref_id).unwrap_or_default();
            let payload = json!({
                "match_guid": data.match_guid,
                "player": {
                    "name": player_name,
                    "id": player_id,
                },
                "ball": {
                    "location": vector3_value(
                        data.ball.location.x,
                        data.ball.location.y,
                        data.ball.location.z,
                    ),
                    "pre_hit_speed": round_to_i64(data.ball.pre_hit_speed),
                    "post_hit_speed": round_to_i64(data.ball.post_hit_speed),
                }
            });
            vec![SosEnvelope::new("game:ball_hit", payload)]
        }
        StatsEvent::StatfeedEvent(data) => {
            let event_name =
                normalize_statfeed_event_name(&data.event_name, &data.type_label);
            let payload = json!({
                "match_guid": data.match_guid,
                "event_name": event_name,
                "type": data.type_label,
                "main_target": {
                    "name": data.main_target.name,
                    "id": player_ref_id(&data.main_target),
                    "team_num": data.main_target.team_num,
                },
                "secondary_target": data.secondary_target.as_ref().map(|player| {
                    json!({
                        "name": player.name,
                        "id": player_ref_id(player),
                        "team_num": player.team_num,
                    })
                }).unwrap_or(Value::Null),
            });
            vec![SosEnvelope::new("game:statfeed_event", payload)]
        }
        StatsEvent::GoalScored(data) => {
            let assister = data
                .assister
                .as_ref()
                .map(|player| {
                    json!({
                        "name": player.name,
                        "id": player_ref_id(player),
                        "teamnum": player.team_num,
                    })
                })
                .unwrap_or_else(
                    || json!({"name": "", "id": "", "teamnum": Value::Null}),
                );

            let payload = json!({
                "match_guid": data.match_guid,
                "goalspeed": round_to_i64(data.goal_speed),
                "goaltime": data.goal_time,
                "impact_location": {
                    "X": data.impact_location.x,
                    "Y": data.impact_location.y,
                },
                "scorer": {
                    "name": data.scorer.name,
                    "id": player_ref_id(&data.scorer),
                    "teamnum": data.scorer.team_num,
                },
                "assister": assister,
                "ball_last_touch": {
                    "player": player_ref_id(&data.ball_last_touch.player),
                    "speed": round_to_i64(data.ball_last_touch.speed),
                }
            });
            vec![SosEnvelope::new("game:goal_scored", payload)]
        }
        StatsEvent::GoalReplayStart(data) => {
            vec![SosEnvelope::new(
                "game:replay_start",
                match_guid_payload(data),
            )]
        }
        StatsEvent::GoalReplayWillEnd(data) => {
            vec![SosEnvelope::new(
                "game:replay_will_end",
                match_guid_payload(data),
            )]
        }
        StatsEvent::GoalReplayEnd(data) => {
            vec![SosEnvelope::new(
                "game:replay_end",
                match_guid_payload(data),
            )]
        }
        StatsEvent::MatchEnded(data) => {
            let payload = json!({
                "match_guid": data.match_guid,
                "winner_team_num": data.winner_team_num,
            });
            vec![SosEnvelope::new("game:match_ended", payload)]
        }
        StatsEvent::PodiumStart(data) => {
            vec![SosEnvelope::new(
                "game:podium_start",
                match_guid_payload(data),
            )]
        }
        StatsEvent::MatchDestroyed(data) => {
            vec![SosEnvelope::new(
                "game:match_destroyed",
                match_guid_payload(data),
            )]
        }
        StatsEvent::ReplayCreated(data) => {
            vec![SosEnvelope::new(
                "game:replay_created",
                match_guid_payload(data),
            )]
        }
        StatsEvent::RoundStarted(_) => {
            vec![SosEnvelope::new(
                "game:round_started_go",
                Value::String("game_round_started_go".to_string()),
            )]
        }
        _ => {
            vec![passthrough_envelope(event)?]
        }
    };

    Ok(mapped)
}

fn passthrough_envelope(
    event: &StatsEvent,
) -> Result<SosEnvelope, RlStatsError> {
    let value = stats_event_to_value(event)?;
    let source_event = value
        .get("event")
        .and_then(Value::as_str)
        .unwrap_or("Unknown")
        .to_string();
    let source_data = value.get("data").cloned().unwrap_or(Value::Null);

    Ok(SosEnvelope::new(source_event, source_data))
}

fn match_guid_payload(data: &MatchOnlyData) -> Value {
    json!({
        "match_guid": data.match_guid,
    })
}

fn translate_update_state(data: &UpdateStateData) -> Value {
    let players = translate_players(data);
    let teams = translate_teams(&data.game.teams);
    let ball_location =
        extract_location(&data.game.ball.as_ref().map(|ball| &ball.extra));
    let ball_speed = data
        .game
        .ball
        .as_ref()
        .and_then(|ball| ball.speed)
        .map(round_to_i64)
        .unwrap_or_default();
    let ball_team = data
        .game
        .ball
        .as_ref()
        .and_then(|ball| ball.team_num)
        .unwrap_or(255);

    json!({
        "match_guid": data.match_guid,
        "hasGame": true,
        "game": {
            "arena": data.game.arena.clone().unwrap_or_default(),
            "time": data.game.time_seconds.unwrap_or_default(),
            "time_seconds": data.game.time_seconds.unwrap_or_default(),
            "isOT": data.game.b_overtime.unwrap_or(false),
            "isReplay": data.game.b_replay.unwrap_or(false),
            "hasWinner": data.game.b_has_winner.unwrap_or(false),
            "winner": data.game.winner.clone().unwrap_or_default(),
            "hasTarget": data.game.b_has_target.unwrap_or(false),
            "target": data
                .game
                .target
                .as_ref()
                .map(target_ref_value)
                .unwrap_or_default(),
            "ball": {
                "location": ball_location,
                "speed": ball_speed,
                "team": ball_team,
            },
            "teams": teams,
        },
        "players": players,
    })
}

fn translate_players(data: &UpdateStateData) -> Value {
    let mut players = Map::<String, Value>::new();

    for player in &data.players {
        let player_name = player.name.clone().unwrap_or_default();
        let player_shortcut = player
            .shortcut
            .or_else(|| first_i64(&player.extra, &["Shortcut", "shortcut"]))
            .unwrap_or(0);
        let player_id = format_player_id(&player_name, player_shortcut);

        let location = extract_location(&Some(&player.extra));
        let attacker = player
            .attacker
            .as_ref()
            .map(|value| value.name.clone())
            .unwrap_or_default();

        let payload = json!({
            "name": player_name,
            "id": player_id.clone(),
            "primaryID": player.primary_id.clone().unwrap_or_default(),
            "team": player.team_num.unwrap_or_default(),
            "score": player.score.unwrap_or_default(),
            "goals": player.goals.unwrap_or_default(),
            "shots": player.shots.unwrap_or_default(),
            "assists": player.assists.unwrap_or_default(),
            "saves": player.saves.unwrap_or_default(),
            "touches": player.touches.unwrap_or_default(),
            "cartouches": player.car_touches.unwrap_or_default(),
            "demos": player.demos.unwrap_or_default(),
            "boost": player.effective_boost().unwrap_or_default(),
            "speed": player.effective_speed().map(round_to_i64).unwrap_or_default(),
            "hasCar": player.b_has_car.unwrap_or(false),
            "isSonic": player.effective_supersonic().unwrap_or(false),
            "isPowersliding": player.b_powersliding.unwrap_or(false),
            "isDead": player.b_demolished.unwrap_or(false),
            "attacker": attacker,
            "shortcut": player.shortcut.unwrap_or_default(),
            "onGround": player.b_on_ground.unwrap_or(false),
            "onWall": player.b_on_wall.unwrap_or(false),
            "location": location,
        });

        players.insert(player_id, payload);
    }

    Value::Object(players)
}

fn translate_teams(teams: &[TeamState]) -> Value {
    let mut indexed: [Option<&TeamState>; 2] = [None, None];

    for team in teams {
        match team.team_num {
            Some(0) => indexed[0] = Some(team),
            Some(1) => indexed[1] = Some(team),
            _ => {}
        }
    }

    let defaults =
        [("Blue", "1873FF", "E5E5E5"), ("Orange", "C26418", "E5E5E5")];

    let mut output = Vec::with_capacity(2);
    for (index, team) in indexed.into_iter().enumerate() {
        let (default_name, default_primary, default_secondary) =
            defaults[index];
        output.push(match team {
            Some(team) => {
                json!({
                    "name": team.name.clone().unwrap_or_else(|| default_name.to_string()),
                    "score": team.score.unwrap_or_default(),
                    "color_primary": team.color_primary.clone().unwrap_or_else(|| default_primary.to_string()),
                    "color_secondary": team.color_secondary.clone().unwrap_or_else(|| default_secondary.to_string()),
                })
            }
            None => {
                json!({
                    "name": default_name,
                    "score": 0,
                    "color_primary": default_primary,
                    "color_secondary": default_secondary,
                })
            }
        });
    }

    Value::Array(output)
}

fn player_ref_id(player: &PlayerRef) -> String {
    let shortcut = player_ref_shortcut(player).unwrap_or(0);

    format_player_id(&player.name, shortcut)
}

fn target_ref_value(target: &PlayerRef) -> String {
    if player_ref_shortcut(target).is_some() {
        player_ref_id(target)
    } else {
        target.name.clone()
    }
}

fn player_ref_shortcut(player: &PlayerRef) -> Option<i64> {
    if let Some(shortcut) =
        first_i64(&player.extra, &["Shortcut", "shortcut"])
    {
        return Some(shortcut);
    }

    if player.shortcut != 0 {
        Some(player.shortcut)
    } else {
        None
    }
}

fn format_player_id(name: &str, shortcut: i64) -> String {
    format!("{name}_{shortcut}")
}

fn extract_location(extra: &Option<&HashMap<String, Value>>) -> Value {
    let Some(extra) = extra else {
        return location_value(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    };

    if let Some(location) = extra.get("Location").and_then(Value::as_object) {
        return location_from_object(location);
    }

    if let Some(location) = extra.get("location").and_then(Value::as_object) {
        return location_from_object(location);
    }

    location_value(0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
}

fn location_from_object(object: &Map<String, Value>) -> Value {
    let x = value_to_f64(object.get("X")).unwrap_or_default();
    let y = value_to_f64(object.get("Y")).unwrap_or_default();
    let z = value_to_f64(object.get("Z")).unwrap_or_default();
    let pitch = value_to_f64(object.get("pitch")).unwrap_or_default();
    let roll = value_to_f64(object.get("roll")).unwrap_or_default();
    let yaw = value_to_f64(object.get("yaw")).unwrap_or_default();

    location_value(x, y, z, pitch, roll, yaw)
}

fn location_value(
    x: f64,
    y: f64,
    z: f64,
    pitch: f64,
    roll: f64,
    yaw: f64,
) -> Value {
    json!({
        "X": x,
        "Y": y,
        "Z": z,
        "pitch": pitch,
        "roll": roll,
        "yaw": yaw,
    })
}

fn vector3_value(x: f64, y: f64, z: f64) -> Value {
    json!({
        "X": x,
        "Y": y,
        "Z": z,
    })
}

fn first_i64(map: &HashMap<String, Value>, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| {
        map.get(*key).and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str().and_then(|text| text.parse::<i64>().ok()))
        })
    })
}

fn value_to_f64(value: Option<&Value>) -> Option<f64> {
    value.and_then(|value| match value {
        Value::Number(number) => number
            .as_f64()
            .or_else(|| number.as_i64().map(|inner| inner as f64)),
        Value::String(text) => text.parse::<f64>().ok(),
        _ => None,
    })
}

fn round_to_i64(value: f64) -> i64 {
    value.round() as i64
}

fn normalize_statfeed_event_name(event_name: &str, type_label: &str) -> String {
    if event_name.eq_ignore_ascii_case("demolition")
        || (event_name.is_empty() && type_label.eq_ignore_ascii_case("demolish"))
    {
        "Demolish".to_string()
    } else {
        event_name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::translate_stats_event;
    use crate::events::{MatchOnlyData, StatsEvent};

    #[test]
    fn countdown_begin_emits_pre_and_post_events() {
        let event = StatsEvent::CountdownBegin(MatchOnlyData {
            match_guid: Some("ABC".to_string()),
            ..MatchOnlyData::default()
        });

        let envelopes =
            translate_stats_event(&event).expect("translation should succeed");

        assert_eq!(envelopes.len(), 2);
        assert_eq!(envelopes[0].event, "game:pre_countdown_begin");
        assert_eq!(envelopes[1].event, "game:post_countdown_begin");
        assert_eq!(envelopes[0].data["match_guid"], "ABC");
        assert_eq!(envelopes[1].data["match_guid"], "ABC");
    }

    #[test]
    fn round_started_uses_sos_string_payload() {
        let event = StatsEvent::RoundStarted(MatchOnlyData::default());

        let envelopes =
            translate_stats_event(&event).expect("translation should succeed");

        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].event, "game:round_started_go");
        assert_eq!(
            envelopes[0].data,
            Value::String("game_round_started_go".to_string())
        );
    }

    #[test]
    fn unmatched_events_are_passthrough_unknown() {
        let event = StatsEvent::MatchPaused(MatchOnlyData {
            match_guid: Some("M1".to_string()),
            ..MatchOnlyData::default()
        });

        let envelopes =
            translate_stats_event(&event).expect("translation should succeed");

        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].event, "MatchPaused");
        assert_eq!(envelopes[0].data, json!({"MatchGuid": "M1"}));
    }
}
