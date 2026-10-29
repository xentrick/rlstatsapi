use thiserror::Error;

#[derive(Debug, Error)]
pub enum RlStatsError {
    #[error(
        "unable to find TAStatsAPI.ini or DefaultStatsAPI.ini; set ClientOptions.stats_api_ini_path or RL_STATS_API_INI"
    )]
    ConfigPathNotFound,
    #[error("failed to read or write stats API config at {path}: {source}")]
    ConfigIo {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "stats API Port and WebPort are both {port}; the game requires them to differ"
    )]
    PortConflict { port: u16 },
    #[error("invalid stats API command {command}: {reason}")]
    InvalidCommand { command: String, reason: String },
    #[error("socket I/O error: {0}")]
    SocketIo(#[from] std::io::Error),
    #[error("json parse error: {0}")]
    Json(#[from] serde_json::Error),
}
