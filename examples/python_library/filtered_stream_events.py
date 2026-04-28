from rlstats_example import RLStatsStream


def main() -> None:
    stream = RLStatsStream()
    stream.connect()

    try:
        print("Watching goal and match-end events...")
        for event in stream.iter_filtered_events(
            event_types=["goal", "match_ended"],
            limit=25,
        ):
            print(f"{event.event}: {event.data}")
    finally:
        stream.close()


if __name__ == "__main__":
    main()
