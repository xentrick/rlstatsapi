# Changelog

## 0.2.0

Tracks the Rocket League Stats API changes from game patches v2.72 (2026-08-04) and v2.76 (2026-09-22).

### Breaking changes

- `StatsEvent` is now `#[non_exhaustive]` and has new variants (`BoostPickup`, `PlayerJoined`, `PlayerLeft`). Exhaustive matches need a `_ =>` arm.
- `StatsEvent::ReplayCreated` now carries `ReplayCreatedData` (`file_name`, `date`) instead of `MatchOnlyData`.
- `EventKind` has new variants `BoostPickup`, `PlayerJoined` and `PlayerLeft`.
- `MatchSignal::GoalScored` now holds `Box<GoalScoredData>`, which shrinks the enum from 432 to about 80 bytes.
- `ConnectionConfig` has a new `web_port` field.
- `RlStatsError` has new `PortConflict` and `InvalidCommand` variants.

### Added

- Game commands: `StatsCommand` (`ChangePOV`, `LoadReplay`, `SeekReplay`, `SetGameSpeed`, `SetHUDVisibility`, `SetMatchPaused`). Commands are validated locally and sent with `RocketLeagueStatsClient::send_command`.
- Python: `RocketLeagueStatsClient.send_command_json(...)` and `rlstatsapi.list_command_names()`.
- `rl_command` dev tool, which sends one command and echoes the resulting events.
- Typed events `BoostPickup`, `PlayerJoined` and `PlayerLeft`, which are supported in filters, `rl_events` and the Python kind names.
- Typed fields `UpdateStatePlayer::loadout`, `UpdateStatePlayer::pickup_class` and `UpdateStateGame::playlist_id`.
- `MatchEndedData::is_no_contest()`, because the game reports `WinnerTeamNum = -1` for no-contest matches since v2.76. `rl_events` prints `none (no contest)`, and the SOS `game:match_ended` payload gains `no_contest`.
- The SOS `game:replay_created` payload now includes `file_name` and `date`.
- Config: support for the `WebPort` INI key (default `49124`), plus a `PortConflict` error when `Port` equals `WebPort`. Also added `DEFAULT_WEB_PORT`, `STATS_API_INI_FILE` and `STATS_API_INI_FALLBACK_FILE`.

### Fixed

- INI discovery now prefers `TAStatsAPI.ini` over `DefaultStatsAPI.ini`, matching how the game loads its config. Previously, edits could land in the fallback file and be ignored.
- The `mock_broadcast` dev tool failed to compile because it defined `run` twice.

### Notes

- Since v2.76, `GoalScored` also fires during replay playback and carries no replay flag. To tell replay goals apart, check the latest `UpdateState` `Game.bReplay`.
- Live captures (private match and LAN, 2026-10-29) confirmed the `PlayerJoined`/`PlayerLeft` payloads plus `Loadout`, `PickupClass` and `PlaylistId`, with no unmodelled fields. `PickupClass` and loadout slots use the string `"None"` for empty. `BoostPickup` never fired in those sessions, so its payload is still unverified and comes from the documentation tables.

### Tooling

- Added a repository `rustfmt.toml` and reformatted the crate.
- CI now checks formatting, runs clippy with `-D warnings`, and builds and tests with the `dev-tools` feature.
