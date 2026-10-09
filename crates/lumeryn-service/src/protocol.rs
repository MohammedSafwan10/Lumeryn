use lumeryn_core::render::RenderOptions;
use lumeryn_core::{Batch, Error, MediaTools, Project};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::PathBuf, time::Duration};
use uuid::Uuid;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
pub const EVENT_CAPACITY: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolConfig {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub timeout_seconds: u64,
}

impl Default for ToolConfig {
    fn default() -> Self {
        Self {
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
            timeout_seconds: 300,
        }
    }
}

impl TryFrom<ToolConfig> for MediaTools {
    type Error = Error;
    fn try_from(config: ToolConfig) -> Result<Self, Self::Error> {
        if !(1..=3600).contains(&config.timeout_seconds)
            || config.ffmpeg.as_os_str().is_empty()
            || config.ffprobe.as_os_str().is_empty()
        {
            return Err(Error::Invalid("invalid media tool configuration".into()));
        }
        Ok(Self {
            ffmpeg: config.ffmpeg,
            ffprobe: config.ffprobe,
            timeout: Duration::from_secs(config.timeout_seconds),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Create {
        project: Project,
    },
    Inspect,
    History,
    Execute {
        batch: Batch,
        expected_revision: Option<u64>,
        key: Option<String>,
        dry_run: bool,
        tools: ToolConfig,
    },
    Check {
        assets: bool,
    },
    Migrate,
    Backup {
        output: PathBuf,
    },
    Search {
        query: String,
        limit: u32,
    },
    Render {
        output: PathBuf,
        expected_revision: Option<u64>,
        tools: ToolConfig,
        job_id: Uuid,
        #[serde(default)]
        options: RenderOptions,
    },
    Cancel {
        job_id: Uuid,
    },
    Watch {
        after_sequence: Option<u64>,
        wait_ms: u32,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Envelope {
    pub version: u32,
    pub id: Uuid,
    #[serde(deserialize_with = "strict_request")]
    pub request: Request,
}

fn strict_request<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Request, D::Error> {
    let value = Value::deserialize(deserializer)?;
    // Serde's internally tagged unit variants otherwise accept extra map entries even
    // with deny_unknown_fields; enforce their empty payload contract explicitly.
    if value
        .get("operation")
        .and_then(Value::as_str)
        .is_some_and(|name| matches!(name, "inspect" | "history" | "migrate"))
        && value.as_object().is_some_and(|object| object.len() != 1)
    {
        return Err(serde::de::Error::custom(
            "unexpected fields in payload-free request",
        ));
    }
    serde_json::from_value(value).map_err(serde::de::Error::custom)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub sequence: u64,
    pub revision: Option<u64>,
    pub kind: String,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventBatch {
    pub sequence: u64,
    pub snapshot: Option<Project>,
    pub events: Vec<Event>,
    pub resync: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WireError {
    pub code: String,
    pub message: String,
    pub conflict: Option<(u64, u64)>,
}

impl From<Error> for WireError {
    fn from(error: Error) -> Self {
        let conflict = match &error {
            Error::Conflict { expected, current } => Some((*expected, *current)),
            _ => None,
        };
        Self {
            code: error.code().into(),
            message: error.to_string(),
            conflict,
        }
    }
}

impl From<WireError> for Error {
    fn from(error: WireError) -> Self {
        match error.code.as_str() {
            "revision_conflict" => match error.conflict {
                Some((expected, current)) => Self::Conflict { expected, current },
                None => Self::Invalid("invalid conflict response".into()),
            },
            "unsupported" => Self::Unsupported(error.message),
            "not_found" => Self::NotFound(error.message),
            "already_exists" => Self::Exists(error.message),
            "media_failure" => Self::Media(error.message),
            "timeout" => Self::Timeout,
            "io_failure" => Self::Io(std::io::Error::other(error.message)),
            "storage_failure" => Self::Storage(sqlx::Error::Protocol(error.message)),
            _ => Self::Invalid(error.message),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Response {
    pub version: u32,
    pub id: Uuid,
    pub result: Result<Value, WireError>,
}

#[cfg(test)]
mod export_tests {
    use super::*;
    #[test]
    fn old_render_request_defaults_options_without_changing_wire_version()
    -> Result<(), Box<dyn std::error::Error>> {
        let request: Request = serde_json::from_value(
            serde_json::json!({"operation":"render","output":"new.mp4","expected_revision":null,"tools":ToolConfig::default(),"job_id":Uuid::new_v4()}),
        )?;
        match request {
            Request::Render { options, .. } => assert_eq!(options, RenderOptions::default()),
            _ => return Err("wrong parsed request".into()),
        }
        Ok(())
    }
}
