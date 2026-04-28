from rlstats_example import RLStatsStream


def main() -> None:
    stream = RLStatsStream()
    stream.connect()

    try:
        for event in stream.iter_events(limit=25):
            print(f"{event.event}: {event.data}")
    finally:
        stream.close()


if __name__ == "__main__":
    main()
