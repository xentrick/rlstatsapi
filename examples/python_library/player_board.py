from __future__ import annotations

import argparse
import sys
import time
from typing import Any

from rlstats_example import RLStatsStream


def pick(mapping: dict[str, Any], *keys: str, default: Any = None) -> Any:
    for key in keys:
        if key in mapping:
            return mapping[key]
    return default


def to_int(value: Any, fallback: int = -1) -> int:
    if isinstance(value, bool):
        return int(value)
    if isinstance(value, (int, float)):
        return int(value)
    return fallback


def fmt_int(value: Any) -> str:
    if isinstance(value, bool):
        return str(int(value))
    if isinstance(value, (int, float)):
        return str(int(value))
    return "-"


def fmt_speed(value: Any) -> str:
    if isinstance(value, (int, float)):
        return f"{value:.0f}"
    return "-"


def short_primary_id(value: Any) -> str:
    if not isinstance(value, str) or not value:
        return "-"
    return value.split("|")[-1]


def team_score(teams: list[dict[str, Any]], team_num: int) -> int:
    for team in teams:
        if to_int(pick(team, "TeamNum", "team_num"), fallback=-1) == team_num:
            return to_int(pick(team, "Score", "score"), fallback=-1)
    return -1


def render_board(data: dict[str, Any]) -> None:
    players = pick(data, "Players", "players", default=[])
    game = pick(data, "Game", "game", default={})

    if not isinstance(players, list):
        players = []
    if not isinstance(game, dict):
        game = {}

    teams = pick(game, "Teams", "teams", default=[])
    if not isinstance(teams, list):
        teams = []

    blue = team_score(teams, 0)
    orange = team_score(teams, 1)
    frame = to_int(pick(game, "Frame", "frame"), fallback=-1)
    time_seconds = to_int(
        pick(game, "TimeSeconds", "time_seconds"),
        fallback=-1,
    )
    match_guid = pick(data, "MatchGuid", "match_guid", default="-")

    table_rows: list[dict[str, Any]] = []
    for raw_player in players:
        if isinstance(raw_player, dict):
            table_rows.append(raw_player)

    table_rows.sort(
        key=lambda player: (
            to_int(pick(player, "TeamNum", "team_num"), fallback=-1),
            str(pick(player, "Name", "name", default="")).lower(),
        )
    )

    sys.stdout.write("\x1b[2J\x1b[H")
    sys.stdout.write(
        f"Match={match_guid}  Frame={frame}  Time={time_seconds}s  "
        f"Score={blue}-{orange}  Players={len(table_rows)}\n"
    )
    sys.stdout.write(
        f"{'Team':<4} {'Player':<18} {'Primary':<7} {'Score':>6} {'G':>5} {'A':>5} "
        f"{'S':>5} {'Shots':>6} {'Touch':>6} {'Boost':>6} {'Speed':>7}\n"
    )
    sys.stdout.write(
        f"{'-' * 4:<4} {'-' * 18:<18} {'-' * 7:<7} {'-' * 6:>6} {'-' * 5:>5} {'-' * 5:>5} "
        f"{'-' * 5:>5} {'-' * 6:>6} {'-' * 6:>6} {'-' * 6:>6} {'-' * 7:>7}\n"
    )

    for player in table_rows:
        sys.stdout.write(
            f"{fmt_int(pick(player, 'TeamNum', 'team_num')):<4} "
            f"{str(pick(player, 'Name', 'name', default='-')):<18} "
            f"{short_primary_id(pick(player, 'PrimaryId', 'primary_id')):<7} "
            f"{fmt_int(pick(player, 'Score', 'score')):>6} "
            f"{fmt_int(pick(player, 'Goals', 'goals')):>5} "
            f"{fmt_int(pick(player, 'Assists', 'assists')):>5} "
            f"{fmt_int(pick(player, 'Saves', 'saves')):>5} "
            f"{fmt_int(pick(player, 'Shots', 'shots')):>6} "
            f"{fmt_int(pick(player, 'Touches', 'touches')):>6} "
            f"{fmt_int(pick(player, 'Boost', 'boost')):>6} "
            f"{fmt_speed(pick(player, 'Speed', 'speed')):>7}\n"
        )

    sys.stdout.flush()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Render all players in-place from UpdateState events (no scrolling spam)."
        )
    )
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=49123)
    parser.add_argument("--ini-path")
    parser.add_argument(
        "--refresh-ms",
        type=int,
        default=200,
        help="Minimum milliseconds between redraws",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()

    stream = RLStatsStream(
        host=args.host,
        port=args.port,
        ini_path=args.ini_path,
    )
    stream.connect()

    last_render = 0.0
    refresh_seconds = max(args.refresh_ms, 0) / 1000.0

    try:
        for event in stream.iter_filtered_events(event_types=["update_state"]):
            now = time.monotonic()
            if now - last_render < refresh_seconds:
                continue

            render_board(event.data)
            last_render = now
    except KeyboardInterrupt:
        pass
    finally:
        stream.close()
        print("\nplayer board stopped")


if __name__ == "__main__":
    main()
