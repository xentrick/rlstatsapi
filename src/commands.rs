use serde_json::{Map, Value, json};

use crate::error::RlStatsError;

/// Camera perspectives accepted by the `ChangePOV` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Perspective {
    Fly,
    SoftAttach,
    HardAttach,
    PlayerView,
    AutoCam,
    CameraDirector,
}

impl Perspective {
    pub const ALL: [Perspective; 6] = [
        Perspective::Fly,
        Perspective::SoftAttach,
        Perspective::HardAttach,
        Perspective::PlayerView,
        Perspective::AutoCam,
        Perspective::CameraDirector,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Perspective::Fly => "Fly",
            Perspective::SoftAttach => "SoftAttach",
            Perspective::HardAttach => "HardAttach",
            Perspective::PlayerView => "PlayerView",
            Perspective::AutoCam => "AutoCam",
            Perspective::CameraDirector => "Camera_Director",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|perspective| perspective.as_str() == name)
    }
}

/// Camera target for `ChangePOV`: the ball, or a player by spectator shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PovFocus {
    Ball,
    Player(u32),
}

impl PovFocus {
    fn to_wire(self) -> String {
        match self {
            PovFocus::Ball => "Ball".to_string(),
            PovFocus::Player(shortcut) => shortcut.to_string(),
        }
    }

    fn from_wire(raw: &str) -> Option<Self> {
        if raw == "Ball" {
            return Some(PovFocus::Ball);
        }
        if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        raw.parse::<u32>().ok().map(PovFocus::Player)
    }
}

/// Commands the game accepts back over the Stats API socket (added in v2.72).
///
/// The game silently drops malformed commands, so [`StatsCommand::to_json`]
/// validates locally first.
#[derive(Debug, Clone, PartialEq)]
pub enum StatsCommand {
    ChangePov {
        focus: Option<PovFocus>,
        perspective: Option<Perspective>,
    },
    LoadReplay {
        file_name: Option<String>,
        path: Option<String>,
    },
    SeekReplay {
        frame: Option<u64>,
        time_seconds: Option<f64>,
    },
    SetGameSpeed {
        speed: f64,
    },
    SetHudVisibility {
        visible: bool,
    },
    SetMatchPaused {
        paused: bool,
    },
}

impl StatsCommand {
    pub const NAMES: [&'static str; 6] = [
        "ChangePOV",
        "LoadReplay",
        "SeekReplay",
        "SetGameSpeed",
        "SetHUDVisibility",
        "SetMatchPaused",
    ];

    pub fn name(&self) -> &'static str {
        match self {
            StatsCommand::ChangePov { .. } => "ChangePOV",
            StatsCommand::LoadReplay { .. } => "LoadReplay",
            StatsCommand::SeekReplay { .. } => "SeekReplay",
            StatsCommand::SetGameSpeed { .. } => "SetGameSpeed",
            StatsCommand::SetHudVisibility { .. } => "SetHUDVisibility",
            StatsCommand::SetMatchPaused { .. } => "SetMatchPaused",
        }
    }

    pub fn validate(&self) -> Result<(), RlStatsError> {
        let invalid = |reason: &str| {
            Err(RlStatsError::InvalidCommand {
                command: self.name().to_string(),
                reason: reason.to_string(),
            })
        };

        match self {
            StatsCommand::ChangePov {
                focus: None,
                perspective: None,
            } => invalid("at least one of Focus or Perspective is required"),
            StatsCommand::LoadReplay { file_name, path }
                if is_blank(file_name) && is_blank(path) =>
            {
                invalid("one of FileName or Path is required")
            }
            StatsCommand::SeekReplay {
                frame: None,
                time_seconds: None,
            } => invalid("one of Frame or TimeSeconds is required"),
            StatsCommand::SeekReplay {
                time_seconds: Some(time),
                ..
            } if !time.is_finite() || *time < 0.0 => {
                invalid("TimeSeconds must be a finite, non-negative number")
            }
            StatsCommand::SetGameSpeed { speed }
                if !speed.is_finite() || *speed < 0.0 =>
            {
                invalid("Speed must be a finite, non-negative number")
            }
            _ => Ok(()),
        }
    }

    pub fn to_value(&self) -> Result<Value, RlStatsError> {
        self.validate()?;

        let mut data = Map::new();
        match self {
            StatsCommand::ChangePov { focus, perspective } => {
                if let Some(focus) = focus {
                    data.insert("Focus".into(), json!(focus.to_wire()));
                }
                if let Some(perspective) = perspective {
                    data.insert(
                        "Perspective".into(),
                        json!(perspective.as_str()),
                    );
                }
            }
            StatsCommand::LoadReplay { file_name, path } => {
                if let Some(file_name) =
                    file_name.as_deref().filter(|v| !v.is_empty())
                {
                    data.insert("FileName".into(), json!(file_name));
                }
                if let Some(path) = path.as_deref().filter(|v| !v.is_empty()) {
                    data.insert("Path".into(), json!(path));
                }
            }
            StatsCommand::SeekReplay {
                frame,
                time_seconds,
            } => {
                if let Some(frame) = frame {
                    data.insert("Frame".into(), json!(frame));
                }
                if let Some(time_seconds) = time_seconds {
                    data.insert("TimeSeconds".into(), json!(time_seconds));
                }
            }
            StatsCommand::SetGameSpeed { speed } => {
                data.insert("Speed".into(), json!(speed));
            }
            StatsCommand::SetHudVisibility { visible } => {
                data.insert("bVisible".into(), json!(visible));
            }
            StatsCommand::SetMatchPaused { paused } => {
                data.insert("bPaused".into(), json!(paused));
            }
        }

        Ok(json!({"Command": self.name(), "Data": data}))
    }

    pub fn to_json(&self) -> Result<String, RlStatsError> {
        Ok(serde_json::to_string(&self.to_value()?)?)
    }

    /// Parses the wire shape `{"Command": ..., "Data": {...}}` and validates it.
    pub fn from_value(value: &Value) -> Result<Self, RlStatsError> {
        let name =
            value
                .get("Command")
                .and_then(Value::as_str)
                .ok_or_else(|| RlStatsError::InvalidCommand {
                    command: String::new(),
                    reason: "missing Command string".to_string(),
                })?;
        let empty = Map::new();
        let data = value
            .get("Data")
            .and_then(Value::as_object)
            .unwrap_or(&empty);
        let invalid = |reason: String| RlStatsError::InvalidCommand {
            command: name.to_string(),
            reason,
        };

        let command = match name {
            "ChangePOV" => {
                let focus = match data.get("Focus") {
                    None => None,
                    Some(raw) => Some(
                        raw.as_str()
                            .and_then(PovFocus::from_wire)
                            .ok_or_else(|| {
                                invalid(format!(
                                    "Focus must be \"Ball\" or a spectator shortcut written as digits, received {raw}"
                                ))
                            })?,
                    ),
                };
                let perspective = match data.get("Perspective") {
                    None => None,
                    Some(raw) => Some(
                        raw.as_str().and_then(Perspective::from_name).ok_or_else(
                            || {
                                invalid(format!(
                                    "Perspective must be one of {}, received {raw}",
                                    Perspective::ALL.map(Perspective::as_str).join(", ")
                                ))
                            },
                        )?,
                    ),
                };
                StatsCommand::ChangePov { focus, perspective }
            }
            "LoadReplay" => StatsCommand::LoadReplay {
                file_name: optional_string(data, "FileName", &invalid)?,
                path: optional_string(data, "Path", &invalid)?,
            },
            "SeekReplay" => StatsCommand::SeekReplay {
                frame: match data.get("Frame") {
                    None => None,
                    Some(raw) => Some(raw.as_u64().ok_or_else(|| {
                        invalid("Frame must be a non-negative integer".into())
                    })?),
                },
                time_seconds: optional_f64(data, "TimeSeconds", &invalid)?,
            },
            "SetGameSpeed" => StatsCommand::SetGameSpeed {
                speed: optional_f64(data, "Speed", &invalid)?
                    .ok_or_else(|| invalid("Speed is required".into()))?,
            },
            "SetHUDVisibility" => StatsCommand::SetHudVisibility {
                visible: required_bool(data, "bVisible", &invalid)?,
            },
            "SetMatchPaused" => StatsCommand::SetMatchPaused {
                paused: required_bool(data, "bPaused", &invalid)?,
            },
            _ => {
                return Err(invalid(format!(
                    "unsupported command; expected one of {}",
                    Self::NAMES.join(", ")
                )));
            }
        };

        command.validate()?;
        Ok(command)
    }
}

fn is_blank(value: &Option<String>) -> bool {
    value.as_deref().is_none_or(str::is_empty)
}

fn optional_string(
    data: &Map<String, Value>,
    key: &str,
    invalid: &impl Fn(String) -> RlStatsError,
) -> Result<Option<String>, RlStatsError> {
    match data.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(invalid(format!("{key} must be a string"))),
    }
}

fn optional_f64(
    data: &Map<String, Value>,
    key: &str,
    invalid: &impl Fn(String) -> RlStatsError,
) -> Result<Option<f64>, RlStatsError> {
    match data.get(key) {
        None => Ok(None),
        Some(raw) => raw
            .as_f64()
            .map(Some)
            .ok_or_else(|| invalid(format!("{key} must be a number"))),
    }
}

fn required_bool(
    data: &Map<String, Value>,
    key: &str,
    invalid: &impl Fn(String) -> RlStatsError,
) -> Result<bool, RlStatsError> {
    data.get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| invalid(format!("{key} must be a boolean")))
}
