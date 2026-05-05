use std::fs;
use std::path::PathBuf;

use rlstatsapi::{parse_stats_event, translate_stats_event};
use serde_json::{Value, json};

fn fixture_path(file_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("json")
        .join("parsed_events")
        .join(file_name)
}

fn read_fixture_events(file_name: &str) -> Vec<Value> {
    let path = fixture_path(file_name);
    let raw = fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!("failed to read fixture {}: {err}", path.display())
    });
    serde_json::from_str::<Vec<Value>>(&raw).unwrap_or_else(|err| {
        panic!("invalid fixture JSON {}: {err}", path.display())
    })
}

#[test]
fn update_state_translation_uses_sos_shape_and_player_ids() {
    let events = read_fixture_events("UpdateState.json");
    let event = parse_stats_event(&events[0].to_string())
        .expect("fixture event should parse");

    let envelopes =
        translate_stats_event(&event).expect("translation should succeed");

    assert_eq!(envelopes.len(), 1);
    assert_eq!(envelopes[0].event, "game:update_state");

    let payload = &envelopes[0].data;
    assert_eq!(payload["match_guid"], "1F7ED23011F1435166EBAB919DB5566D");
    assert_eq!(payload["game"]["teams"][0]["name"], "Blue");
    assert_eq!(payload["game"]["teams"][1]["score"], 1);
    assert_eq!(payload["game"]["ball"]["team"], 1);
        assert_eq!(payload["game"]["target"], "nickm_1");

    assert_eq!(payload["players"]["nickm_1"]["name"], "nickm");
    assert_eq!(payload["players"]["Zone Killa_5"]["name"], "Zone Killa");
}

#[test]
fn update_state_target_falls_back_to_name_without_shortcut() {
        let event_json = json!({
            "Event": "UpdateState",
            "Data": {
                "MatchGuid": "M-Target-1",
                "Game": {
                    "Teams": [],
                    "bHasTarget": true,
                    "Target": {
                        "Name": "Ball",
                        "TeamNum": 0
                    }
                }
            }
        });

        let event =
                parse_stats_event(&event_json.to_string()).expect("event should parse");
        let envelopes =
                translate_stats_event(&event).expect("translation should succeed");

        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].event, "game:update_state");
        assert_eq!(envelopes[0].data["game"]["target"], "Ball");
}

#[test]
fn update_state_target_defaults_to_empty_string_when_missing() {
        let event_json = json!({
            "Event": "UpdateState",
            "Data": {
                "MatchGuid": "M-Target-2",
                "Game": {
                    "Teams": [],
                    "bHasTarget": false
                }
            }
        });

        let event =
                parse_stats_event(&event_json.to_string()).expect("event should parse");
        let envelopes =
                translate_stats_event(&event).expect("translation should succeed");

        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].event, "game:update_state");
        assert_eq!(envelopes[0].data["game"]["target"], "");
}

#[test]
fn goal_scored_translation_uses_sos_field_names() {
    let event_json = json!({
      "Event": "GoalScored",
      "Data": {
        "MatchGuid": "M-1",
        "GoalSpeed": 2010.3,
        "GoalTime": 45.0,
        "ImpactLocation": {"X": 0.2, "Y": 0.7, "Z": 0.0},
                "Scorer": {"Name": "A", "PrimaryId": "Steam|123|0", "Shortcut": 1, "TeamNum": 0},
                "Assister": {"Name": "B", "PrimaryId": "Steam|456|0", "Shortcut": 3, "TeamNum": 0},
        "BallLastTouch": {
                    "Player": {"Name": "A", "PrimaryId": "Steam|123|0", "Shortcut": 1, "TeamNum": 0},
          "Speed": 1980.0
        }
      }
    });

    let event =
        parse_stats_event(&event_json.to_string()).expect("event should parse");
    let envelopes =
        translate_stats_event(&event).expect("translation should succeed");

    assert_eq!(envelopes[0].event, "game:goal_scored");
    assert_eq!(envelopes[0].data["goalspeed"], 2010);
    assert_eq!(envelopes[0].data["scorer"]["id"], "A_1");
    assert_eq!(envelopes[0].data["assister"]["id"], "B_3");
    assert_eq!(envelopes[0].data["ball_last_touch"]["player"], "A_1");
}

#[test]
fn ball_hit_translation_uses_player_ref_id_format() {
        let event_json = json!({
            "Event": "BallHit",
            "Data": {
                "MatchGuid": "M-BH-1",
                "Players": [
                    {"Name": "A", "Shortcut": 7, "TeamNum": 0}
                ],
                "Ball": {
                    "PreHitSpeed": 980.0,
                    "PostHitSpeed": 1520.0,
                    "Location": {"X": 1.0, "Y": 2.0, "Z": 3.0}
                }
            }
        });

        let event =
                parse_stats_event(&event_json.to_string()).expect("event should parse");
        let envelopes =
                translate_stats_event(&event).expect("translation should succeed");

        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].event, "game:ball_hit");
        assert_eq!(envelopes[0].data["player"]["id"], "A_7");
}

#[test]
fn statfeed_demolition_event_name_normalizes_to_demolish() {
    let event_json = json!({
      "Event": "StatfeedEvent",
      "Data": {
        "MatchGuid": "M-3",
                "EventName": "Demolition",
                "Type": "Demolish",
                "MainTarget": {"Name": "A", "PrimaryId": "Steam|123|0", "Shortcut": 1, "TeamNum": 0},
                "SecondaryTarget": {"Name": "B", "PrimaryId": "Steam|456|0", "Shortcut": 3, "TeamNum": 1}
      }
    });

    let event =
        parse_stats_event(&event_json.to_string()).expect("event should parse");
    let envelopes =
        translate_stats_event(&event).expect("translation should succeed");

    assert_eq!(envelopes.len(), 1);
    assert_eq!(envelopes[0].event, "game:statfeed_event");
    assert_eq!(envelopes[0].data["event_name"], "Demolish");
    assert_eq!(envelopes[0].data["type"], "Demolish");
    assert_eq!(envelopes[0].data["main_target"]["id"], "A_1");
    assert_eq!(envelopes[0].data["secondary_target"]["id"], "B_3");
}

#[test]
fn statfeed_missing_secondary_target_uses_sos_empty_target() {
    let event_json = json!({
      "Event": "StatfeedEvent",
      "Data": {
        "MatchGuid": "M-4",
        "EventName": "Save",
        "Type": "Save",
        "MainTarget": {"Name": "A", "PrimaryId": "Steam|123|0", "Shortcut": 1, "TeamNum": 0}
      }
    });

    let event =
        parse_stats_event(&event_json.to_string()).expect("event should parse");
    let envelopes =
        translate_stats_event(&event).expect("translation should succeed");

    assert_eq!(envelopes.len(), 1);
    assert_eq!(envelopes[0].event, "game:statfeed_event");
    assert_eq!(envelopes[0].data["secondary_target"]["name"], "");
    assert_eq!(envelopes[0].data["secondary_target"]["id"], "");
    assert_eq!(envelopes[0].data["secondary_target"]["team_num"], -1);
}

#[test]
fn empty_match_created_guid_translates_as_missing_until_initialized() {
    let created_json = json!({
        "Event": "MatchCreated",
        "Data": {
            "MatchGuid": ""
        }
    });

    let created_event = parse_stats_event(&created_json.to_string())
        .expect("event should parse");
    let created_envelopes = translate_stats_event(&created_event)
        .expect("translation should succeed");

    assert_eq!(created_envelopes.len(), 1);
    assert_eq!(created_envelopes[0].event, "game:match_created");
    assert_eq!(created_envelopes[0].data["match_guid"], Value::Null);

    let initialized_json = json!({
        "Event": "MatchInitialized",
        "Data": {
            "MatchGuid": "M-REAL-1"
        }
    });

    let initialized_event = parse_stats_event(&initialized_json.to_string())
        .expect("event should parse");
    let initialized_envelopes = translate_stats_event(&initialized_event)
        .expect("translation should succeed");

    assert_eq!(initialized_envelopes.len(), 1);
    assert_eq!(initialized_envelopes[0].event, "game:initialized");
    assert_eq!(initialized_envelopes[0].data["match_guid"], "M-REAL-1");
}

#[test]
fn update_state_ball_location_defaults_to_origin_when_missing() {
    let event_json = json!({
        "Event": "UpdateState",
        "Data": {
            "MatchGuid": "M-BALL-1",
            "Game": {
                "Teams": [],
                "Ball": {
                    "Speed": 1000.0,
                    "TeamNum": 1
                }
            },
            "Players": []
        }
    });

    let event =
        parse_stats_event(&event_json.to_string()).expect("event should parse");
    let envelopes =
        translate_stats_event(&event).expect("translation should succeed");

    assert_eq!(envelopes.len(), 1);
    assert_eq!(envelopes[0].event, "game:update_state");
    assert_eq!(
        envelopes[0].data["game"]["ball"]["location"]["X"].as_f64(),
        Some(0.0)
    );
    assert_eq!(
        envelopes[0].data["game"]["ball"]["location"]["Y"].as_f64(),
        Some(0.0)
    );
    assert_eq!(
        envelopes[0].data["game"]["ball"]["location"]["Z"].as_f64(),
        Some(0.0)
    );
    assert_eq!(envelopes[0].data["game"]["ball"]["speed"], 1000);
    assert_eq!(envelopes[0].data["game"]["ball"]["team"], 1);
}

#[test]
fn unknown_passthrough_preserves_source_event_name_and_data() {
    let event_json = json!({
      "Event": "ClockUpdatedSeconds",
      "Data": {
        "MatchGuid": "M-2",
        "TimeSeconds": 120,
        "bOvertime": false
      }
    });

    let event =
        parse_stats_event(&event_json.to_string()).expect("event should parse");
    let envelopes =
        translate_stats_event(&event).expect("translation should succeed");

    assert_eq!(envelopes.len(), 1);
    assert_eq!(envelopes[0].event, "ClockUpdatedSeconds");
    assert_eq!(envelopes[0].data["TimeSeconds"], 120);
    assert_eq!(envelopes[0].data["bOvertime"], false);
}

#[test]
fn replay_will_end_alias_translates_to_sos_replay_will_end() {
        let event_json = json!({
            "Event": "ReplayWillEnd",
            "Data": {
                "MatchGuid": "M-RWE-1"
            }
        });

        let event =
                parse_stats_event(&event_json.to_string()).expect("event should parse");
        let envelopes =
                translate_stats_event(&event).expect("translation should succeed");

        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].event, "game:replay_will_end");
        assert_eq!(envelopes[0].data["match_guid"], "M-RWE-1");
}

#[test]
fn replay_start_and_end_aliases_translate_to_sos_replay_events() {
    let replay_start_json = json!({
        "Event": "ReplayStart",
        "Data": {
            "MatchGuid": "M-RS-1"
        }
    });

    let replay_start_event = parse_stats_event(&replay_start_json.to_string())
        .expect("event should parse");
    let replay_start_envelopes = translate_stats_event(&replay_start_event)
        .expect("translation should succeed");

    assert_eq!(replay_start_envelopes.len(), 1);
    assert_eq!(replay_start_envelopes[0].event, "game:replay_start");
    assert_eq!(replay_start_envelopes[0].data["match_guid"], "M-RS-1");

    let replay_end_json = json!({
        "Event": "ReplayEnd",
        "Data": {
            "MatchGuid": "M-RE-1"
        }
    });

    let replay_end_event = parse_stats_event(&replay_end_json.to_string())
        .expect("event should parse");
    let replay_end_envelopes =
        translate_stats_event(&replay_end_event).expect("translation should succeed");

    assert_eq!(replay_end_envelopes.len(), 1);
    assert_eq!(replay_end_envelopes[0].event, "game:replay_end");
    assert_eq!(replay_end_envelopes[0].data["match_guid"], "M-RE-1");
}

#[test]
fn replay_transition_goal_scored_with_empty_scorer_is_kept() {
    let event_json = json!({
        "Event": "GoalScored",
        "Data": {
            "MatchGuid": "FA65D41E11F148B23539FCB7688033D6",
            "GoalSpeed": 0.0,
            "GoalTime": 0,
            "ImpactLocation": {
                "X": 712.02734375,
                "Y": -5334.61181640625,
                "Z": 92.61934661865235
            },
            "Scorer": { "Name": "", "Shortcut": 0, "TeamNum": 0 },
            "BallLastTouch": {
                "Player": { "Name": "nickm", "Shortcut": 5, "TeamNum": 1 },
                "Speed": 90.86030578613281
            }
        }
    });

    let event =
        parse_stats_event(&event_json.to_string()).expect("event should parse");
    let envelopes =
        translate_stats_event(&event).expect("translation should succeed");

    assert_eq!(envelopes.len(), 1);
    assert_eq!(envelopes[0].event, "game:goal_scored");
    assert_eq!(envelopes[0].data["scorer"]["name"], "");
    assert_eq!(envelopes[0].data["scorer"]["id"], "");
    assert_eq!(envelopes[0].data["goalspeed"], 0);
    assert_eq!(envelopes[0].data["goaltime"], 0.0);
}
