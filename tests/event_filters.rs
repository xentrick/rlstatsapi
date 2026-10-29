use rlstatsapi::{
    EventFilter, EventKind, MatchSignal, PlayerTracker, StatsEvent,
    parse_stats_event, to_match_signal, winner_team_num,
};
use serde_json::json;

fn parse(event_name: &str, data: serde_json::Value) -> StatsEvent {
    let payload = json!({
        "Event": event_name,
        "Data": data,
    });

    parse_stats_event(&payload.to_string()).expect("event should parse")
}

#[test]
fn event_filter_can_select_specific_event_kind() {
    let update = parse("UpdateState", json!({"Players": [], "Game": {}}));
    let goal = parse(
        "GoalScored",
        json!({
            "MatchGuid": "M1",
            "ImpactLocation": {"X": 0, "Y": 0, "Z": 0},
            "Scorer": {"Name": "Alice", "Shortcut": 1, "TeamNum": 0},
            "BallLastTouch": {"Player": {"Name": "Alice", "Shortcut": 1, "TeamNum": 0}}
        }),
    );

    let filter = EventFilter::new().include_kind(EventKind::GoalScored);

    assert!(!filter.matches(&update));
    assert!(filter.matches(&goal));
}

#[test]
fn event_filter_can_match_player_and_match_guid() {
    let update = parse(
        "UpdateState",
        json!({
            "MatchGuid": "M42",
            "Players": [
                {
                    "Name": "Alice",
                    "PrimaryId": "Steam|123|0",
                    "TeamNum": 0,
                    "Boost": 56,
                    "Score": 12,
                    "Touches": 8
                }
            ],
            "Game": {
                "Frame": 10,
                "TimeSeconds": 250,
                "Teams": [
                    {"TeamNum": 0, "Score": 1},
                    {"TeamNum": 1, "Score": 2}
                ]
            }
        }),
    );

    let filter = EventFilter::new()
        .with_match_guid("M42")
        .with_player_name("alice")
        .with_player_primary_id("Steam|123|0");

    assert!(filter.matches(&update));

    let mismatch = EventFilter::new().with_match_guid("OTHER");
    assert!(!mismatch.matches(&update));
}

#[test]
fn player_tracker_emits_only_when_snapshot_changes() {
    let update = parse(
        "UpdateState",
        json!({
            "MatchGuid": "M42",
            "Players": [
                {
                    "Name": "Alice",
                    "PrimaryId": "Steam|123|0",
                    "TeamNum": 0,
                    "Boost": 56,
                    "Score": 12,
                    "Touches": 8
                }
            ],
            "Game": {
                "Frame": 10,
                "TimeSeconds": 250,
                "Teams": [
                    {"TeamNum": 0, "Score": 1},
                    {"TeamNum": 1, "Score": 2}
                ]
            }
        }),
    );

    let mut tracker = PlayerTracker::by_name("Alice");

    let first = tracker
        .update_from_event(&update)
        .expect("first snapshot should be emitted");
    assert_eq!(first.name, "Alice");
    assert_eq!(first.boost, Some(56));
    assert_eq!(first.frame, Some(10));

    let second = tracker.update_from_event(&update);
    assert!(second.is_none(), "unchanged snapshot should not be emitted");
}

#[test]
fn match_signal_helpers_detect_goals_and_match_end() {
    let goal = parse(
        "GoalScored",
        json!({
            "MatchGuid": "M1",
            "ImpactLocation": {"X": 0, "Y": 0, "Z": 0},
            "Scorer": {"Name": "Alice", "Shortcut": 1, "TeamNum": 0},
            "BallLastTouch": {"Player": {"Name": "Alice", "Shortcut": 1, "TeamNum": 0}}
        }),
    );

    let ended = parse(
        "MatchEnded",
        json!({
            "MatchGuid": "M1",
            "WinnerTeamNum": 1
        }),
    );

    match to_match_signal(&goal) {
        Some(MatchSignal::GoalScored(data)) => {
            assert_eq!(data.scorer.name, "Alice");
        }
        other => panic!("unexpected goal signal: {other:?}"),
    }

    match to_match_signal(&ended) {
        Some(MatchSignal::MatchConcluded(data)) => {
            assert_eq!(data.winner_team_num, 1);
        }
        other => panic!("unexpected match-ended signal: {other:?}"),
    }

    assert_eq!(winner_team_num(&ended), Some(1));
}

#[test]
fn match_signal_keeps_replay_transition_goal_scored() {
    let transition_goal = parse(
        "GoalScored",
        json!({
            "MatchGuid": "M1",
            "GoalSpeed": 0.0,
            "GoalTime": 0,
            "ImpactLocation": {"X": 0, "Y": 0, "Z": 0},
            "Scorer": {"Name": "", "Shortcut": 0, "TeamNum": 0},
            "BallLastTouch": {"Player": {"Name": "Alice", "Shortcut": 1, "TeamNum": 0}}
        }),
    );

    match to_match_signal(&transition_goal) {
        Some(MatchSignal::GoalScored(data)) => {
            assert_eq!(data.goal_speed, 0.0);
            assert_eq!(data.goal_time, 0.0);
            assert_eq!(data.scorer.name, "");
        }
        other => panic!("unexpected goal signal: {other:?}"),
    }
}

#[test]
fn nested_car_payload_still_exposes_player_boost_and_speed() {
    let update = parse(
        "UpdateState",
        json!({
            "MatchGuid": "M77",
            "Players": [
                {
                    "Name": "Local",
                    "PrimaryId": "Steam|111|0",
                    "TeamNum": 0,
                    "Boost": 57,
                    "Speed": 1450,
                    "Score": 100
                },
                {
                    "Name": "Remote",
                    "PrimaryId": "Steam|222|0",
                    "TeamNum": 1,
                    "Score": 50,
                    "Car": {
                        "BoostAmount": 33,
                        "CarSpeed": 1320,
                        "bBoosting": true,
                        "bSupersonic": false
                    }
                }
            ],
            "Game": {
                "Frame": 400,
                "TimeSeconds": 120,
                "Teams": [
                    {"TeamNum": 0, "Score": 1},
                    {"TeamNum": 1, "Score": 1}
                ]
            }
        }),
    );

    let StatsEvent::UpdateState(data) = &update else {
        panic!("expected update state");
    };

    let remote = data
        .players
        .iter()
        .find(|player| player.name.as_deref() == Some("Remote"))
        .expect("remote player should be present");

    assert_eq!(remote.boost, None);
    assert_eq!(remote.speed, None);
    assert_eq!(remote.effective_boost(), Some(33));
    assert_eq!(remote.effective_speed(), Some(1320.0));
    assert_eq!(remote.effective_boosting(), Some(true));
    assert_eq!(remote.effective_supersonic(), Some(false));

    let mut tracker = PlayerTracker::by_name("Remote");
    let snapshot = tracker
        .update_from_event(&update)
        .expect("tracker should emit first snapshot");
    assert_eq!(snapshot.boost, Some(33));
    assert_eq!(snapshot.speed, Some(1320.0));
}

#[test]
fn event_filter_matches_v2_72_player_events() {
    let boost = parse(
        "BoostPickup",
        json!({
            "MatchGuid": "M1",
            "Player": {"Name": "Alice", "Shortcut": 1, "TeamNum": 0},
            "BoostAmount": 12,
            "BoostType": "BoostType_Pad"
        }),
    );
    let joined = parse(
        "PlayerJoined",
        json!({"MatchGuid": "M1", "PlayerName": "Bob", "PrimaryId": "Epic|b|0"}),
    );
    let left = parse(
        "PlayerLeft",
        json!({"MatchGuid": "M1", "PlayerName": "Bob", "PrimaryId": "Epic|b|0"}),
    );

    assert_eq!(EventKind::from(&boost), EventKind::BoostPickup);
    assert_eq!(EventKind::from(&joined), EventKind::PlayerJoined);
    assert_eq!(EventKind::from(&left), EventKind::PlayerLeft);

    let by_team = EventFilter::new().with_team_num(0);
    assert!(by_team.matches(&boost));
    assert!(!EventFilter::new().with_team_num(1).matches(&boost));

    let by_name = EventFilter::new().with_player_name("bob");
    assert!(by_name.matches(&joined));
    assert!(by_name.matches(&left));
    assert!(!by_name.matches(&boost));

    let by_id = EventFilter::new().with_player_primary_id("Epic|b|0");
    assert!(by_id.matches(&joined));

    let by_match = EventFilter::new().with_match_guid("M1");
    assert!(by_match.matches(&boost));
    assert!(by_match.matches(&left));
}
