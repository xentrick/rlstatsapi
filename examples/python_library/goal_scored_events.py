from rlstats_example import RLStatsStream


def main() -> None:
    stream = RLStatsStream()
    stream.connect()

    try:
        print("Watching goal_scored events...")
        for event in stream.iter_filtered_events(
            event_types=["goal"],
            limit=25,
        ):
            data = event.data
            scorer = data.get("scorer", {}).get("name", "unknown")
            assister_data = data.get("assister") or {}
            assister = assister_data.get("name") or "-"
            speed = data.get("goal_speed", 0)
            goal_time = data.get("goal_time", 0)
            match_guid = data.get("match_guid", "-")
            print(
                f"GOAL scorer={scorer} assister={assister} "
                f"speed={speed} time={goal_time} match={match_guid}"
            )
    finally:
        stream.close()


if __name__ == "__main__":
    main()
