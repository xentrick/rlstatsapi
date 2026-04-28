pub mod client;
pub mod config;
pub mod error;
pub mod events;
#[cfg(feature = "python")]
mod python_bindings;

pub use client::RocketLeagueStatsClient;
pub use config::{
    ClientOptions, ConnectionConfig, DEFAULT_PACKET_SEND_RATE, DEFAULT_PORT,
    MAX_PACKET_SEND_RATE, discover_default_stats_api_ini_path,
    prepare_connection_config,
};
pub use error::RlStatsError;
pub use events::{
    EventEnvelope, StatsEvent, parse_stats_event, parse_stats_event_value,
    stats_event_name, stats_event_to_value,
};
