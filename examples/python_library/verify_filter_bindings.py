from __future__ import annotations

import json

import rlstatsapi
from rlstats_example import RLStatsStream


GOAL_RAW = json.dumps(
    {
        "Event": "GoalScored",
        "Data": {
            "MatchGuid": "M1",
            "GoalSpeed": 87.3,
            "GoalTime": 127.5,
            "ImpactLocation": {"X": 0, "Y": -2944, "Z": 320},
            "Scorer": {
                "Name": "PlayerA",
                "Shortcut": 1,
                "TeamNum": 0,
            },
            "BallLastTouch": {
                "Player": {
                    "Name": "PlayerA",
                    "Shortcut": 1,
                    "TeamNum": 0,
                }
            },
        },
    }
)

MATCH_ENDED_RAW = json.dumps(
    {
        "Event": "MatchEnded",
        "Data": {
            "MatchGuid": "M1",
            "WinnerTeamNum": 1,
        },
    }
)

UPDATE_RAW = json.dumps(
    {
        "Event": "UpdateState",
        "Data": {
            "MatchGuid": "M1",
            "Players": [
                {
                    "Name": "PlayerA",
                    "PrimaryId": "Steam|123|0",
                    "TeamNum": 0,
                    "Score": 350,
                    "Goals": 2,
                    "Touches": 22,
                    "Boost": 44,
                }
            ],
            "Game": {
                "Frame": 120,
                "TimeSeconds": 180,
                "Teams": [
                    {"TeamNum": 0, "Score": 2},
                    {"TeamNum": 1, "Score": 1},
                ],
            },
        },
    }
)


def main() -> None:
    kinds = rlstatsapi.list_event_kinds()
    assert "goal_scored" in kinds
    assert "match_ended" in kinds

    assert rlstatsapi.event_matches(GOAL_RAW, event_types=["goal"]) is True
    assert (
        rlstatsapi.event_matches(
            GOAL_RAW,
            event_types=["goal"],
            player_name="playera",
        )
        is True
    )
    assert (
        rlstatsapi.event_matches(
            GOAL_RAW,
            event_types=["goal"],
            team_num=1,
        )
        is False
    )

    filtered = rlstatsapi.filter_event_json(
        UPDATE_RAW,
        event_types=["update_state"],
        player_name="PlayerA",
        player_primary_id="Steam|123|0",
        match_guid="M1",
    )
    assert filtered is not None
    payload = json.loads(filtered)
    assert payload["event"] == "UpdateState"

    assert rlstatsapi.winner_team(MATCH_ENDED_RAW) == 1

    signal = rlstatsapi.match_signal_json(MATCH_ENDED_RAW)
    assert signal is not None
    signal_payload = json.loads(signal)
    assert signal_payload["signal"] == "match_concluded"

    wrapped = RLStatsStream.filter_event(
        GOAL_RAW,
        event_types=["goal"],
        player_name="PlayerA",
    )
    assert wrapped is not None
    assert wrapped.event == "GoalScored"

    wrapped_signal = RLStatsStream.match_signal(MATCH_ENDED_RAW)
    assert wrapped_signal is not None
    assert wrapped_signal.signal == "match_concluded"

    print("filter bindings verified")


if __name__ == "__main__":
    main()
