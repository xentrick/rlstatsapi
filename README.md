# rlstatsapi

Rust client and parser for Rocket League Stats API event streams over TCP, with optional Python bindings via PyO3.

## What This Project Includes

- TCP client with reconnect support for Rocket League Stats API events.
- Typed event parsing (`StatsEvent`) with support for unknown/forward-compatible events.
- Config helpers for optional Stats API INI handling.
- CLI binaries for raw, compact tick, and pretty filtered output.
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

### 3) Pretty event listener with filters

```bash
cargo run --bin pretty_events -- --help
cargo run --bin pretty_events -- --list-events
cargo run --bin pretty_events -- --event goal
```

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

## Example Python Library

A working Python consumer package is included at:

- `examples/python_library`

Run it with `uv`:

```bash
cd examples/python_library
uv sync
uv run demo.py
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
