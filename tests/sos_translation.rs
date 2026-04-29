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
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read fixture {}: {err}", path.display()));
    serde_json::from_str::<Vec<Value>>(&raw)
        .unwrap_or_else(|err| panic!("invalid fixture JSON {}: {err}", path.display()))
}

#[test]
fn update_state_translation_uses_sos_shape_and_player_ids() {
    let events = read_fixture_events("UpdateState.json");
    let event = parse_stats_event(&events[0].to_string()).expect("fixture event should parse");

    let envelopes = translate_stats_event(&event).expect("translation should succeed");

    assert_eq!(envelopes.len(), 1);
    assert_eq!(envelopes[0].event, "game:update_state");

    let payload = &envelopes[0].data;
    assert_eq!(payload["match_guid"], "1F7ED23011F1435166EBAB919DB5566D");
    assert_eq!(payload["game"]["teams"][0]["name"], "Blue");
    assert_eq!(payload["game"]["teams"][1]["score"], 1);
    assert_eq!(payload["game"]["ball"]["team"], 1);

    assert_eq!(
        payload["players"]["Steam|76561197960409023|0"]["name"],
        "nickm"
    );
    assert_eq!(
        payload["players"]["Epic|9dfa38bf97f9442997b464ea869eb96a|0"]["name"],
        "Zone Killa"
    );
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
        "Scorer": {"Name": "A", "PrimaryId": "Steam|123|0", "TeamNum": 0},
        "Assister": {"Name": "B", "PrimaryId": "Steam|456|0", "TeamNum": 0},
        "BallLastTouch": {
          "Player": {"Name": "A", "PrimaryId": "Steam|123|0", "TeamNum": 0},
          "Speed": 1980.0
        }
      }
    });

    let event = parse_stats_event(&event_json.to_string()).expect("event should parse");
    let envelopes = translate_stats_event(&event).expect("translation should succeed");

    assert_eq!(envelopes[0].event, "game:goal_scored");
    assert_eq!(envelopes[0].data["goalspeed"], 2010);
    assert_eq!(envelopes[0].data["scorer"]["id"], "Steam|123|0");
    assert_eq!(envelopes[0].data["assister"]["id"], "Steam|456|0");
    assert_eq!(envelopes[0].data["ball_last_touch"]["player"], "Steam|123|0");
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

    let event = parse_stats_event(&event_json.to_string()).expect("event should parse");
    let envelopes = translate_stats_event(&event).expect("translation should succeed");

    assert_eq!(envelopes.len(), 1);
    assert_eq!(envelopes[0].event, "ClockUpdatedSeconds");
    assert_eq!(envelopes[0].data["TimeSeconds"], 120);
    assert_eq!(envelopes[0].data["bOvertime"], false);
}
