use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::RlStatsError;

pub const DEFAULT_PORT: u16 = 49123;
pub const DEFAULT_PACKET_SEND_RATE: f32 = 60.0;
pub const MAX_PACKET_SEND_RATE: f32 = 120.0;
const DEFAULT_INI_TEMPLATE: &str = "[TAGame.MatchStatsExporter_TA]\n\n; Port the client will listen for connections on\nPort=49123\n\n; How many times per second the game sends the update state (capped at 120, 0 disables this feature)\nPacketSendRate=60\n";

#[derive(Debug, Clone)]
pub struct ClientOptions {
    pub host: String,
    pub port_override: Option<u16>,
    pub stats_api_ini_path: Option<PathBuf>,
    pub auto_enable_packet_rate: bool,
    pub packet_send_rate: f32,
    pub set_packet_rate_only_when_zero: bool,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port_override: None,
            stats_api_ini_path: None,
            auto_enable_packet_rate: true,
            packet_send_rate: DEFAULT_PACKET_SEND_RATE,
            set_packet_rate_only_when_zero: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub host: String,
    pub port: u16,
    pub packet_send_rate: f32,
    pub ini_path: Option<PathBuf>,
    pub ini_mutated: bool,
}

impl ConnectionConfig {
    pub fn socket_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

pub fn prepare_connection_config(
    options: &ClientOptions,
) -> Result<ConnectionConfig, RlStatsError> {
    let ini_path = resolve_ini_path(options)?;
    let mut port = options.port_override.unwrap_or(DEFAULT_PORT);
    let mut packet_send_rate = clamp_rate(options.packet_send_rate);
    let mut ini_mutated = false;

    if let Some(path) = &ini_path {
        let mut ini = read_config_file(path)?;

        if options.port_override.is_none() {
            let parsed_port = find_u16_value(&ini, "Port").filter(|p| *p > 0);

            if let Some(valid_port) = parsed_port {
                port = valid_port;
            } else {
                ini = upsert_key(&ini, "Port", &port.to_string());
                ini_mutated = true;
            }
        }

        if options.auto_enable_packet_rate {
            let existing_rate =
                find_f32_value(&ini, "PacketSendRate").unwrap_or(0.0);
            let should_set = if options.set_packet_rate_only_when_zero {
                existing_rate <= 0.0
            } else {
                true
            };

            if should_set {
                ini = upsert_key(
                    &ini,
                    "PacketSendRate",
                    &format_rate(packet_send_rate),
                );
                ini_mutated = true;
            } else {
                packet_send_rate = clamp_rate(existing_rate);
            }

            if ini_mutated {
                write_config_file(path, &ini)?;
            }
        } else if ini_mutated {
            write_config_file(path, &ini)?;
        }

        if let Some(rate) = find_f32_value(&ini, "PacketSendRate") {
            packet_send_rate = clamp_rate(rate);
        }
    }

    Ok(ConnectionConfig {
        host: options.host.clone(),
        port,
        packet_send_rate,
        ini_path,
        ini_mutated,
    })
}

fn resolve_ini_path(
    options: &ClientOptions,
) -> Result<Option<PathBuf>, RlStatsError> {
    if let Some(path) = &options.stats_api_ini_path {
        ensure_ini_file_exists(path)?;
        return Ok(Some(path.clone()));
    }

    Ok(None)
}

pub fn discover_default_stats_api_ini_path() -> Option<PathBuf> {
    default_stats_api_ini_candidates()
        .into_iter()
        .find(|path| path.exists())
}

fn default_stats_api_ini_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(from_env) = env::var("RL_STATS_API_INI") {
        candidates.push(PathBuf::from(from_env));
    }

    if let Ok(home) = env::var("HOME") {
        let home_path = PathBuf::from(home);

        candidates.push(
            home_path
                .join(".local/share/Steam/steamapps/common/rocketleague/TAGame/Config/DefaultStatsAPI.ini"),
        );
        candidates.push(
            home_path
                .join(".steam/steam/steamapps/common/rocketleague/TAGame/Config/DefaultStatsAPI.ini"),
        );
        candidates.push(
            home_path.join(
                ".steam/steam/steamapps/compatdata/252950/pfx/drive_c/users/steamuser/Documents/My Games/Rocket League/TAGame/Config/DefaultStatsAPI.ini",
            ),
        );
    }

    if let Ok(program_files) = env::var("ProgramFiles") {
        candidates.push(
            PathBuf::from(program_files)
                .join("Rocket League/TAGame/Config/DefaultStatsAPI.ini"),
        );
    }

    if let Ok(program_files_x86) = env::var("ProgramFiles(x86)") {
        candidates.push(
            PathBuf::from(program_files_x86)
                .join("Rocket League/TAGame/Config/DefaultStatsAPI.ini"),
        );
    }

    if let Ok(app_data) = env::var("APPDATA") {
        candidates.push(PathBuf::from(app_data).join(
            "..\\Local\\Rocket League\\TAGame\\Config\\DefaultStatsAPI.ini",
        ));
    }

    candidates
}

fn ensure_ini_file_exists(path: &Path) -> Result<(), RlStatsError> {
    if path.exists() {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| RlStatsError::ConfigIo {
            path: parent.display().to_string(),
            source,
        })?;
    }

    fs::write(path, DEFAULT_INI_TEMPLATE).map_err(|source| RlStatsError::ConfigIo {
        path: path.display().to_string(),
        source,
    })
}

fn read_config_file(path: &Path) -> Result<String, RlStatsError> {
    fs::read_to_string(path).map_err(|source| RlStatsError::ConfigIo {
        path: path.display().to_string(),
        source,
    })
}

fn write_config_file(path: &Path, contents: &str) -> Result<(), RlStatsError> {
    fs::write(path, contents).map_err(|source| RlStatsError::ConfigIo {
        path: path.display().to_string(),
        source,
    })
}

fn clamp_rate(rate: f32) -> f32 {
    rate.clamp(0.0, MAX_PACKET_SEND_RATE)
}

fn format_rate(rate: f32) -> String {
    let mut formatted = format!("{rate:.3}");
    while formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    if formatted.is_empty() {
        "0".to_string()
    } else {
        formatted
    }
}

fn find_f32_value(contents: &str, key: &str) -> Option<f32> {
    find_string_value(contents, key).and_then(|raw| raw.parse::<f32>().ok())
}

fn find_u16_value(contents: &str, key: &str) -> Option<u16> {
    find_string_value(contents, key).and_then(|raw| raw.parse::<u16>().ok())
}

fn find_string_value(contents: &str, key: &str) -> Option<String> {
    for line in contents.lines() {
        if let Some((lhs, rhs, _indent)) = split_key_value(line) {
            if lhs.eq_ignore_ascii_case(key) {
                return Some(rhs.to_string());
            }
        }
    }
    None
}

fn upsert_key(contents: &str, key: &str, value: &str) -> String {
    let mut found = false;
    let mut output_lines = Vec::new();

    for line in contents.lines() {
        if let Some((lhs, _rhs, indent)) = split_key_value(line) {
            if lhs.eq_ignore_ascii_case(key) {
                output_lines.push(format!("{indent}{key}={value}"));
                found = true;
                continue;
            }
        }
        output_lines.push(line.to_string());
    }

    if !found {
        if !output_lines.is_empty()
            && !output_lines.last().is_some_and(|line| line.is_empty())
        {
            output_lines.push(String::new());
        }
        output_lines.push(format!("{key}={value}"));
    }

    output_lines.join("\n")
}

fn split_key_value(line: &str) -> Option<(&str, &str, &str)> {
    let indent_len = line.chars().take_while(|ch| ch.is_whitespace()).count();
    let indent = &line[..indent_len];
    let trimmed = line[indent_len..].trim();

    if trimmed.is_empty()
        || trimmed.starts_with(';')
        || trimmed.starts_with('#')
        || trimmed.starts_with('[')
    {
        return None;
    }

    let (lhs, rhs) = trimmed.split_once('=')?;
    Some((lhs.trim(), rhs.trim(), indent))
}
