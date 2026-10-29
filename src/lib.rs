//! Rust client and parser for the Rocket League Stats API.
//!
//! The game streams JSON events over a local TCP socket (default
//! `127.0.0.1:49123`) once `PacketSendRate` is set in `TAStatsAPI.ini`.
//! [`RocketLeagueStatsClient`] connects to that socket, parses each message
//! into a typed [`StatsEvent`] (unrecognised events become
//! [`StatsEvent::Unknown`]), and can send [`StatsCommand`]s back to the game.
//!
//! ```no_run
//! use rlstatsapi::{ClientOptions, RocketLeagueStatsClient, StatsCommand, StatsEvent};
//!
//! # async fn example() -> Result<(), rlstatsapi::RlStatsError> {
//! let mut client = RocketLeagueStatsClient::connect(ClientOptions::default()).await?;
//!
//! while let Some(event) = client.next_event().await? {
//!     if let StatsEvent::GoalScored(goal) = &event {
//!         println!("{} scored at {:.1} kph", goal.scorer.name, goal.goal_speed);
//!         client
//!             .send_command(&StatsCommand::SetGameSpeed { speed: 0.5 })
//!             .await?;
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Other building blocks:
//!
//! - [`EventFilter`], [`PlayerTracker`] and [`to_match_signal`] for selecting
//!   and summarising events.
//! - [`translate_stats_event`] for mapping events to SOS overlay messages.
//! - [`prepare_connection_config`] for reading or updating the game's INI file.

pub mod client;
pub mod commands;
pub mod config;
pub mod console;
pub mod error;
pub mod events;
pub mod filters;
#[cfg(feature = "python")]
mod python_bindings;
pub mod sos;

pub use client::RocketLeagueStatsClient;
pub use commands::{Perspective, PovFocus, StatsCommand};
pub use config::{
    ClientOptions, ConnectionConfig, DEFAULT_PACKET_SEND_RATE, DEFAULT_PORT,
    DEFAULT_WEB_PORT, MAX_PACKET_SEND_RATE, STATS_API_INI_FALLBACK_FILE,
    STATS_API_INI_FILE, discover_default_stats_api_ini_path,
    prepare_connection_config,
};
pub use error::RlStatsError;
pub use events::{
    EventEnvelope, StatsEvent, is_bogus_goal_scored, parse_stats_event,
    parse_stats_event_value, stats_event_name, stats_event_to_value,
};
pub use filters::{
    EventFilter, EventKind, MatchSignal, PlayerSnapshot, PlayerTracker,
    to_match_signal, winner_team_num,
};
pub use sos::{
    SOS_VERSION, SosEnvelope, sos_version_envelope, translate_stats_event,
};
