# rlstats-example-lib

This is a small example Python library that consumes the `rlstatsapi` PyO3 bindings.

## Prerequisites

- Python 3.11+
- `uv`

This example is configured to resolve `rlstatsapi` from the local repository path via `[tool.uv.sources]`.

## Install the Example Library

```bash
cd examples/python_library
uv sync
```

## Run the Basic Example

```bash
uv run stream_events.py
```

This script connects to `127.0.0.1:49123` by default and prints normalized event objects.

Filtered stream examples (requires live Stats API stream):

```bash
uv run filtered_stream_events.py
uv run goal_scored_events.py
uv run player_watch.py
uv run player_board.py
```

Offline verification example (no live game required):

```bash
uv run verify_filter_bindings.py
```

## Sending Commands to the Game

Commands use the Stats API wire shape (`{"Command": ..., "Data": {...}}`) and are validated before they are sent, because the game silently ignores malformed commands:

```python
stream = RLStatsStream()
stream.connect()
stream.send_command({"Command": "SetMatchPaused", "Data": {"bPaused": True}})
```

- `RocketLeagueStatsClient.send_command_json(raw_json)`
- `RLStatsStream.send_command(command_dict)`
- `rlstatsapi.list_command_names()`: `ChangePOV`, `LoadReplay`, `SeekReplay`, `SetGameSpeed`, `SetHUDVisibility`, `SetMatchPaused`

## Python Filtering API

Client-side filtered polling:

- `RocketLeagueStatsClient.next_filtered_event_json(...)`
- `RLStatsStream.next_filtered_event(...)`
- `RLStatsStream.iter_filtered_events(...)`

Module-level filtering helpers:

- `rlstatsapi.list_event_kinds()`
- `rlstatsapi.event_matches(...)`
- `rlstatsapi.filter_event_json(...)`
- `rlstatsapi.match_signal_json(...)`
- `rlstatsapi.winner_team(...)` (returns `-1` for a no-contest match on game v2.76+)

`list_event_kinds()` includes the game v2.72 events `boost_pickup`, `player_joined` and `player_left`.
