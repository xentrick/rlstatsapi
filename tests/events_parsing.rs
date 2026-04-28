use rlstatsapi::{parse_stats_event, StatsEvent};
use serde_json::json;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn variant_name(event: &StatsEvent) -> &'static str {
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

fn parse(event_name: &str, data: serde_json::Value) -> StatsEvent {
    let payload = json!({
        "Event": event_name,
        "Data": data,
    });

    parse_stats_event(&payload.to_string()).expect("event should parse")
}

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
    let events: Vec<Value> = serde_json::from_str(&raw)
        .unwrap_or_else(|err| panic!("invalid fixture JSON {}: {err}", path.display()));
    assert!(!events.is_empty(), "fixture {} is empty", path.display());
    events
}

#[test]
fn parses_all_documented_event_variants() {
    let cases = vec![
        ("UpdateState", json!({"Players": [], "Game": {}})),
        (
            "BallHit",
            json!({"Ball": {"Location": {"X": 0, "Y": 0, "Z": 0}}}),
        ),
        ("ClockUpdatedSeconds", json!({})),
        ("CountdownBegin", json!({})),
        ("CrossbarHit", json!({"BallLocation": {"X": 0, "Y": 0, "Z": 0}})),
        ("GoalReplayEnd", json!({})),
        ("GoalReplayStart", json!({})),
        ("GoalReplayWillEnd", json!({})),
        ("GoalScored", json!({"ImpactLocation": {"X": 0, "Y": 0, "Z": 0}})),
        ("MatchCreated", json!({})),
        ("MatchInitialized", json!({})),
        ("MatchDestroyed", json!({})),
        ("MatchEnded", json!({})),
        ("MatchPaused", json!({})),
        ("MatchUnpaused", json!({})),
        ("PodiumStart", json!({})),
        ("ReplayCreated", json!({})),
        ("RoundStarted", json!({})),
        ("StatfeedEvent", json!({})),
    ];

    for (name, data) in cases {
        let event = parse(name, data);
        assert_eq!(variant_name(&event), name);
    }
}

#[test]
fn keeps_unknown_events_for_forward_compatibility() {
    let event = parse("FutureEventName", json!({"SomeField": 1}));

    match event {
        StatsEvent::Unknown(unknown) => {
            assert_eq!(unknown.event, "FutureEventName");
            assert_eq!(unknown.data["SomeField"], 1);
        }
        _ => panic!("expected unknown event variant"),
    }
}

#[test]
fn parses_when_data_is_nested_json_string() {
    let payload = r#"{"Event":"RoundStarted","Data":"{\"MatchGuid\":\"ABC123\"}"}"#;

    let event = parse_stats_event(payload).expect("event should parse");

    match event {
        StatsEvent::RoundStarted(data) => {
            assert_eq!(data.match_guid.as_deref(), Some("ABC123"));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn parses_generated_parsed_event_fixtures() {
    let fixture_files = [
        "UpdateState.json",
        "BallHit.json",
        "ClockUpdatedSeconds.json",
    ];

    for file_name in fixture_files {
        let events = read_fixture_events(file_name);

        for event_value in events {
            let expected_name = event_value
                .get("Event")
                .and_then(Value::as_str)
                .expect("fixture event should contain Event string");

            let parsed = parse_stats_event(&event_value.to_string())
                .expect("fixture event should parse");

            assert_eq!(variant_name(&parsed), expected_name);
        }
    }
}

#[test]
fn parsed_update_state_player_fixtures_cover_multiple_players() {
    let player_fixtures = [
        ("UpdateState_nickm.json", "nickm"),
        ("UpdateState_zone_killa.json", "Zone Killa"),
    ];

    for (file_name, expected_player_name) in player_fixtures {
        let events = read_fixture_events(file_name);

        for event_value in events {
            let parsed = parse_stats_event(&event_value.to_string())
                .expect("player fixture event should parse");

            match parsed {
                StatsEvent::UpdateState(data) => {
                    assert!(
                        data.players.iter().any(|player| {
                            player.name.as_deref() == Some(expected_player_name)
                        }),
                        "expected player {expected_player_name} in fixture {file_name}"
                    );
                }
                other => panic!("expected UpdateState fixture, got {other:?}"),
            }
        }
    }
}
