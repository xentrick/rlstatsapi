use thiserror::Error;

#[derive(Debug, Error)]
pub enum RlStatsError {
    #[error(
        "unable to find DefaultStatsAPI.ini; set ClientOptions.stats_api_ini_path or RL_STATS_API_INI"
    )]
    ConfigPathNotFound,
    #[error("failed to read or write stats API config at {path}: {source}")]
    ConfigIo {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("socket I/O error: {0}")]
    SocketIo(#[from] std::io::Error),
    #[error("json parse error: {0}")]
    Json(#[from] serde_json::Error),
}
