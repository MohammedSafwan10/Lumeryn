//! Proprietary shared project service. Engine state lives here, never in UI widgets.
mod client;
mod protocol;
mod server;
#[cfg(windows)]
mod windows;

pub use client::Client;
pub use protocol::{
    EVENT_CAPACITY, Event, EventBatch, ImportFailure, ImportJob, ImportSource, ImportSourceStatus,
    ImportStage, MAX_FRAME_BYTES, PROTOCOL_VERSION, Request, ToolConfig,
};
pub use server::serve;

use lumeryn_core::{Error, Result};
use serde::{Serialize, de::DeserializeOwned};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub(crate) fn absolute_project(path: &Path) -> Result<PathBuf> {
    if path.as_os_str().is_empty() {
        return Err(Error::Invalid("empty project path".into()));
    }
    if path.exists() {
        return Ok(std::fs::canonicalize(path)?);
    }
    let absolute = std::path::absolute(path)?;
    let parent = absolute
        .parent()
        .ok_or_else(|| Error::Invalid("project must have a parent".into()))?;
    let name = absolute
        .file_name()
        .ok_or_else(|| Error::Invalid("project must have a filename".into()))?;
    Ok(std::fs::canonicalize(parent)?.join(name))
}

pub(crate) async fn write_frame<T: Serialize>(
    writer: &mut (impl AsyncWrite + Unpin),
    value: &T,
) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(Error::Invalid("IPC response exceeds 8 MiB limit".into()));
    }
    writer.write_u32_le(bytes.len() as u32).await?;
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

pub(crate) async fn read_frame<T: DeserializeOwned>(
    reader: &mut (impl AsyncRead + Unpin),
) -> Result<T> {
    let size = reader.read_u32_le().await? as usize;
    if size == 0 || size > MAX_FRAME_BYTES {
        return Err(Error::Invalid("IPC frame length must be 1..8 MiB".into()));
    }
    let mut bytes = vec![0; size];
    reader.read_exact(&mut bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    #[tokio::test]
    async fn bounded_frames_roundtrip_and_reject_length_and_unknown_fields() -> Result<()> {
        let (mut sender, mut receiver) = tokio::io::duplex(1024);
        write_frame(&mut sender, &json!({"revision":12})).await?;
        let actual: Value = read_frame(&mut receiver).await?;
        assert_eq!(actual["revision"], 12);
        for size in [0, MAX_FRAME_BYTES as u32 + 1, u32::MAX] {
            sender.write_u32_le(size).await?;
            assert!(matches!(
                read_frame::<Value>(&mut receiver).await,
                Err(Error::Invalid(_))
            ));
        }
        let invalid = json!({"version":1,"id":uuid::Uuid::new_v4(),"request":{"operation":"inspect","untrusted_extra":true}});
        write_frame(&mut sender, &invalid).await?;
        assert!(matches!(
            read_frame::<protocol::Envelope>(&mut receiver).await,
            Err(Error::Json(_))
        ));
        Ok(())
    }
}
