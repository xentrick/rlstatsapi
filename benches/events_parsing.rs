use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rlstatsapi::parse_stats_event;

const UPDATE_STATE_PAYLOAD: &str = r#"{
  "Event": "UpdateState",
  "Data": {
    "MatchGuid": "A1B2C3D4",
    "Players": [
      {
        "Name": "PlayerA",
        "PrimaryId": "Steam|123|0",
        "Shortcut": 1,
        "TeamNum": 0,
        "Score": 125,
        "Goals": 1,
        "Shots": 2,
        "Assists": 0,
        "Saves": 1,
        "Touches": 14,
        "CarTouches": 3,
        "Demos": 0,
        "bHasCar": true,
        "Speed": 1200,
        "Boost": 45,
        "bBoosting": true,
        "bOnGround": true,
        "bOnWall": false,
        "bPowersliding": false,
        "bDemolished": false,
        "bSupersonic": false
      }
    ],
    "Game": {
      "Teams": [
        {
          "Name": "Blue",
          "TeamNum": 0,
          "Score": 1,
          "ColorPrimary": "0000FF",
          "ColorSecondary": "0000AA"
        },
        {
          "Name": "Orange",
          "TeamNum": 1,
          "Score": 0,
          "ColorPrimary": "FF7F00",
          "ColorSecondary": "AA5500"
        }
      ],
      "TimeSeconds": 180,
      "bOvertime": false,
      "Frame": 120,
      "Elapsed": 50.2,
      "Ball": {
        "Speed": 850.5,
        "TeamNum": 0
      },
      "bReplay": false,
      "bHasWinner": false,
      "Winner": "",
      "Arena": "Stadium_P",
      "bHasTarget": true,
      "Target": {
        "Name": "PlayerA",
        "Shortcut": 1,
        "TeamNum": 0
      }
    }
  }
}"#;

const GOAL_SCORED_PAYLOAD: &str = r#"{
  "Event": "GoalScored",
  "Data": {
    "MatchGuid": "A1B2C3D4",
    "GoalSpeed": 87.3,
    "GoalTime": 127.5,
    "ImpactLocation": {"X": 0, "Y": -2944, "Z": 320},
    "Scorer": {"Name": "PlayerA", "Shortcut": 1, "TeamNum": 0},
    "Assister": {"Name": "PlayerC", "Shortcut": 3, "TeamNum": 0},
    "BallLastTouch": {
      "Player": {"Name": "PlayerA", "Shortcut": 1, "TeamNum": 0},
      "Speed": 125
    }
  }
}"#;

fn bench_parse_update_state(c: &mut Criterion) {
    c.bench_function("parse_update_state", |b| {
        b.iter(|| {
            let event = parse_stats_event(black_box(UPDATE_STATE_PAYLOAD))
                .expect("benchmark payload should parse");
            black_box(event);
        })
    });
}

fn bench_parse_goal_scored(c: &mut Criterion) {
    c.bench_function("parse_goal_scored", |b| {
        b.iter(|| {
            let event = parse_stats_event(black_box(GOAL_SCORED_PAYLOAD))
                .expect("benchmark payload should parse");
            black_box(event);
        })
    });
}

criterion_group!(benches, bench_parse_update_state, bench_parse_goal_scored);
criterion_main!(benches);
