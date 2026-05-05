# rlstatsapi

[![docs](https://img.shields.io/docsrs/rlstatsapi?label=docs)](https://docs.rs/rlstatsapi)
[![crates.io](https://img.shields.io/crates/v/rlstatsapi)](https://crates.io/crates/rlstatsapi)

Rust client and parser for Rocket League Stats API event streams over TCP, with optional Python bindings via PyO3.

## What This Project Includes

- TCP client with reconnect support for Rocket League Stats API events.
- Typed event parsing (`StatsEvent`) with support for unknown/forward-compatible events.
- Event filtering and player/match tracking helpers (`EventFilter`, `PlayerTracker`, `MatchSignal`).
- Config helpers for optional Stats API INI handling.
- CLI binaries for raw, compact tick, and pretty filtered output.
- SOS relay binary that translates RL events to SOS-style websocket events.
- Optional Python extension module (`rlstatsapi`) behind the `python` Cargo feature.
- Example Python library in `examples/python_library`.

## Requirements

- Rust toolchain (stable).
- Python 3.11+ (only needed for Python bindings and examples).
- Rocket League Stats API exporter enabled and sending to `127.0.0.1:49123` (default).

## Build and Test (Rust)

```bash
cargo build
cargo test
```

## Binaries

### 1) Raw event dump

```bash
cargo run --bin raw_events
```

Optional INI path:

```bash
cargo run --bin raw_events -- --ini /path/to/DefaultStatsAPI.ini
```

### 2) Compact tick listener

```bash
cargo run --bin tick_listener
```

### 3) RL event listener with filters

```bash
cargo run --bin rl_events -- --help
cargo run --bin rl_events -- --list-events
cargo run --bin rl_events -- --event goal
```

### 4) Live player board (in-place console updates)

```bash
cargo run --bin player_board
```

### 5) SOS websocket relay (outbound client)

```bash
cargo run --bin sos_relay
```

Send translated SOS events to a custom overlay endpoint:

```bash
cargo run --bin sos_relay -- --ws-host 10.0.0.42 --ws-port 49122
```

Show all options:

```bash
cargo run --bin sos_relay -- --help
```

### 6) SOS websocket broadcast server (middleman)

```bash
cargo run --bin sos_broadcast
```

This mode connects to Rocket League Stats API on localhost and hosts a websocket server for overlays/consumers:

- Input: `127.0.0.1:49123` (default RL stats TCP source)
- Output server bind: `0.0.0.0:49122` (default websocket server)

Connect local consumers to:

```text
ws://localhost:49122
```

Show all options:

```bash
cargo run --bin sos_broadcast -- --help
```

Behavior note:

- `sos_relay` pushes to a remote websocket endpoint.
- `sos_broadcast` hosts the websocket endpoint and broadcasts to connected clients.

### 7) SOS broadcaster GUI (desktop app)

```bash
cargo run --bin sos_broadcast_gui
```

Cross-platform desktop GUI (Linux and Windows) for the broadcaster flow:

- Start/stop broadcasting from the window.
- View live client count, relayed message count, last translated event, and logs.

## Default Connection and INI Behavior

- If you do not pass an INI path, the client uses localhost defaults (`127.0.0.1:49123`) and does not edit an INI file.
- If you pass `--ini <path>`, the INI file is created when missing and updated according to client options.

Expected INI template:

```ini
[TAGame.MatchStatsExporter_TA]

; Port the client will listen for connections on
Port=49123

; How many times per second the game sends the update state (capped at 120, 0 disables this feature)
PacketSendRate=60
```

## Python Bindings (PyO3)

The Python module is built from this crate with the `python` feature.

Rust-side check:

```bash
cargo check --features python
```

Package metadata for Python builds is defined in `pyproject.toml`.

Filtering helpers are exposed for Python users as well, including:

- `RocketLeagueStatsClient.next_filtered_event_json(...)`
- `rlstatsapi.list_event_kinds()`
- `rlstatsapi.event_matches(...)`
- `rlstatsapi.filter_event_json(...)`
- `rlstatsapi.match_signal_json(...)`
- `rlstatsapi.winner_team(...)`

## Example Python Library

A working Python consumer package is included at:

- `examples/python_library`

Run it with `uv`:

```bash
cd examples/python_library
uv sync
uv run stream_events.py
```

Additional examples:

```bash
uv run filtered_stream_events.py
uv run goal_scored_events.py
uv run player_watch.py
uv run player_board.py
uv run verify_filter_bindings.py
```

## Library Usage (Rust)

```rust
use rlstatsapi::{ClientOptions, RocketLeagueStatsClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = ClientOptions::default();
    let mut client = RocketLeagueStatsClient::connect(options).await?;

    while let Some(event) = client.next_event().await? {
        println!("{event:?}");
    }

    Ok(())
}
```

## Automated Releases (GitHub)

This repository now has two release workflows:

- `Release` (`.github/workflows/release.yml`) builds and publishes Rust binaries for:
    - `x86_64-unknown-linux-gnu`
    - `x86_64-pc-windows-msvc`
    - `x86_64-apple-darwin`
    - `aarch64-apple-darwin`
- `Python Wheels` (`.github/workflows/python-wheels.yml`) builds wheels for the same desktop platforms and uploads them to the same GitHub Release.

### Triggering a Release

1. Bump versions as needed.
2. Create and push a version tag:

```bash
git tag v0.1.2
git push origin v0.1.2
```

3. Wait for `Release` to publish binary artifacts.
4. Wait for `Python Wheels` to upload wheel artifacts to that release.

### Optional PyPI Publish

If the repository secret `PYPI_API_TOKEN` is set, the wheel workflow also publishes non-prerelease wheel builds to PyPI when a GitHub Release is published.

### Why Native CI Runners (Not Docker Everywhere)

- Linux wheels use manylinux (Docker-backed) for compatibility.
- Windows and macOS binaries/wheels are built on native runners to avoid brittle cross-toolchain issues.

This gives repeatable Linux artifacts while still producing correct native outputs for Windows and macOS.
