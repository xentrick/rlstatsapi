use rlstatsapi::{StatsEvent, parse_stats_event};
use serde_json::Value;
use serde_json::json;
use std::fs;
use std::path::PathBuf;

fn variant_name(event: &StatsEvent) -> &'static str {
    match event {
        StatsEvent::UpdateState(_) => "UpdateState",
        StatsEvent::BallHit(_) => "BallHit",
        StatsEvent::BoostPickup(_) => "BoostPickup",
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
        StatsEvent::PlayerJoined(_) => "PlayerJoined",
        StatsEvent::PlayerLeft(_) => "PlayerLeft",
        StatsEvent::PodiumStart(_) => "PodiumStart",
        StatsEvent::ReplayCreated(_) => "ReplayCreated",
        StatsEvent::RoundStarted(_) => "RoundStarted",
        StatsEvent::StatfeedEvent(_) => "StatfeedEvent",
        StatsEvent::Unknown(_) => "Unknown",
        _ => panic!("variant_name is missing a StatsEvent variant"),
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
    let raw = fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!("failed to read fixture {}: {err}", path.display())
    });
    let events: Vec<Value> = serde_json::from_str(&raw).unwrap_or_else(|err| {
        panic!("invalid fixture JSON {}: {err}", path.display())
    });
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
        ("BoostPickup", json!({})),
        ("ClockUpdatedSeconds", json!({})),
        ("CountdownBegin", json!({})),
        (
            "CrossbarHit",
            json!({"BallLocation": {"X": 0, "Y": 0, "Z": 0}}),
        ),
        ("GoalReplayEnd", json!({})),
        ("GoalReplayStart", json!({})),
        ("GoalReplayWillEnd", json!({})),
        (
            "GoalScored",
            json!({"ImpactLocation": {"X": 0, "Y": 0, "Z": 0}}),
        ),
        ("MatchCreated", json!({})),
        ("MatchInitialized", json!({})),
        ("MatchDestroyed", json!({})),
        ("MatchEnded", json!({})),
        ("MatchPaused", json!({})),
        ("MatchUnpaused", json!({})),
        ("PlayerJoined", json!({})),
        ("PlayerLeft", json!({})),
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
fn parses_goal_scored_and_goal_replay_variants() {
    let goal_scored = parse(
        "GoalScored",
        json!({
            "MatchGuid": "M-123",
            "ImpactLocation": {"X": 0.1, "Y": 0.2, "Z": 0.0}
        }),
    );
    assert!(matches!(goal_scored, StatsEvent::GoalScored(_)));

    let replay_start = parse("GoalReplayStart", json!({"MatchGuid": "M-123"}));
    assert!(matches!(replay_start, StatsEvent::GoalReplayStart(_)));

    let replay_start_alias =
        parse("ReplayStart", json!({"MatchGuid": "M-123"}));
    assert!(matches!(replay_start_alias, StatsEvent::GoalReplayStart(_)));

    let replay_will_end =
        parse("GoalReplayWillEnd", json!({"MatchGuid": "M-123"}));
    assert!(matches!(replay_will_end, StatsEvent::GoalReplayWillEnd(_)));

    let replay_will_end_alias =
        parse("ReplayWillEnd", json!({"MatchGuid": "M-123"}));
    assert!(matches!(
        replay_will_end_alias,
        StatsEvent::GoalReplayWillEnd(_)
    ));

    let replay_end = parse("GoalReplayEnd", json!({"MatchGuid": "M-123"}));
    assert!(matches!(replay_end, StatsEvent::GoalReplayEnd(_)));

    let replay_end_alias = parse("ReplayEnd", json!({"MatchGuid": "M-123"}));
    assert!(matches!(replay_end_alias, StatsEvent::GoalReplayEnd(_)));
}

#[test]
fn parses_when_data_is_nested_json_string() {
    let payload =
        r#"{"Event":"RoundStarted","Data":"{\"MatchGuid\":\"ABC123\"}"}"#;

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
        "DocumentedV272Events.json",
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

#[test]
fn parses_v2_72_event_payload_fields() {
    let events: Vec<StatsEvent> =
        read_fixture_events("DocumentedV272Events.json")
            .into_iter()
            .map(|value| {
                parse_stats_event(&value.to_string())
                    .expect("fixture should parse")
            })
            .collect();

    match &events[0] {
        StatsEvent::BoostPickup(data) => {
            assert_eq!(data.player.name, "nickm");
            assert_eq!(data.boost_amount, 100.0);
            assert_eq!(data.boost_type, "BoostType_Pill");
            assert!(!data.b_replay);
            assert_eq!(data.location.x, -3072.0);
        }
        other => panic!("expected BoostPickup, got {other:?}"),
    }

    match &events[1] {
        StatsEvent::PlayerJoined(data) => {
            assert_eq!(data.player_name, "Zone Killa");
            assert_eq!(data.primary_id, "Steam|76561198000000000|0");
        }
        other => panic!("expected PlayerJoined, got {other:?}"),
    }

    match &events[3] {
        StatsEvent::ReplayCreated(data) => {
            assert_eq!(data.file_name, "Stadium_P_2026-06-05_18-42");
            assert_eq!(data.date, "2026-06-05 18:42:10");
            assert!(data.extra.is_empty());
        }
        other => panic!("expected ReplayCreated, got {other:?}"),
    }

    match &events[4] {
        StatsEvent::MatchEnded(data) => {
            assert_eq!(data.winner_team_num, -1);
            assert!(data.is_no_contest());
        }
        other => panic!("expected MatchEnded, got {other:?}"),
    }

    match &events[5] {
        StatsEvent::UpdateState(data) => {
            let player = &data.players[0];
            assert_eq!(
                player.loadout.as_deref(),
                Some(
                    &[
                        "Octane".to_string(),
                        "Cristiano".to_string(),
                        "OEM".to_string()
                    ][..]
                )
            );
            assert_eq!(
                player.pickup_class.as_deref(),
                Some("SpecialPickup_BallFreeze")
            );
            assert!(!player.extra.contains_key("Loadout"));
            assert_eq!(data.game.playlist_id, Some(11));
        }
        other => panic!("expected UpdateState, got {other:?}"),
    }
}

#[test]
fn match_ended_with_winner_is_not_no_contest() {
    match parse("MatchEnded", json!({"WinnerTeamNum": 0})) {
        StatsEvent::MatchEnded(data) => assert!(!data.is_no_contest()),
        other => panic!("expected MatchEnded, got {other:?}"),
    }
}
