from rlstats_example import RLStatsStream


PLAYER_NAME = "PlayerA"


def main() -> None:
    stream = RLStatsStream()
    stream.connect()

    try:
        print(f"Watching update_state events for {PLAYER_NAME}...")
        for event in stream.iter_filtered_events(
            event_types=["update_state"],
            player_name=PLAYER_NAME,
            limit=40,
        ):
            players = event.data.get("players", [])
            selected = next(
                (
                    p
                    for p in players
                    if p.get("name", "").lower() == PLAYER_NAME.lower()
                ),
                None,
            )
            if selected is None:
                continue

            boost = selected.get("boost")
            score = selected.get("score")
            goals = selected.get("goals")
            touches = selected.get("touches")
            print(f"score={score} goals={goals} touches={touches} boost={boost}")
    finally:
        stream.close()


if __name__ == "__main__":
    main()
