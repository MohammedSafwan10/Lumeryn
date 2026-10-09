use crate::{
    EventBatch, PROTOCOL_VERSION, Request, ToolConfig,
    protocol::{Envelope, Response},
    read_frame, write_frame,
};
use lumeryn_core::{Batch, Error, Project, Result};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

/// Cloneable stateless client; each request has its own pipe, so watches and edits can overlap.
#[derive(Debug, Clone)]
pub struct Client {
    project: PathBuf,
    auto_launch: bool,
}

impl Client {
    pub async fn connect(project: impl AsRef<Path>) -> Result<Self> {
        let client = Self {
            project: crate::absolute_project(project.as_ref())?,
            auto_launch: true,
        };
        #[cfg(windows)]
        {
            let _pipe = client.open().await?;
        }
        #[cfg(not(windows))]
        {
            return Err(Error::Unsupported(
                "secure local service transport currently requires Windows".into(),
            ));
        }
        #[cfg(windows)]
        Ok(client)
    }
    /// Connect without automatic process launch, useful for embedding/tests with a managed server.
    pub fn existing(project: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            project: crate::absolute_project(project.as_ref())?,
            auto_launch: false,
        })
    }
    pub fn project_path(&self) -> &Path {
        &self.project
    }
    #[cfg(windows)]
    async fn open(&self) -> Result<tokio::net::windows::named_pipe::NamedPipeClient> {
        use tokio::net::windows::named_pipe::ClientOptions;
        let name = crate::windows::pipe_name(&self.project)?;
        let mut launched = false;
        let started = std::time::Instant::now();
        loop {
            match ClientOptions::new().open(&name) {
                Ok(pipe) => return Ok(pipe),
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound
                        || error.raw_os_error() == Some(231) =>
                {
                    if !launched && error.kind() == std::io::ErrorKind::NotFound && self.auto_launch
                    {
                        self.launch()?;
                        launched = true;
                    }
                    if !self.auto_launch && error.kind() == std::io::ErrorKind::NotFound {
                        return Err(Error::Io(error));
                    }
                    if started.elapsed() > Duration::from_secs(10) {
                        return Err(Error::Timeout);
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                Err(error) => return Err(Error::Io(error)),
            }
        }
    }
    fn launch(&self) -> Result<()> {
        let current = std::env::current_exe()?;
        let executable = if current
            .file_stem()
            .is_some_and(|stem| stem.eq_ignore_ascii_case("lumeryn"))
        {
            current
        } else {
            let parent = current
                .parent()
                .ok_or_else(|| Error::Invalid("missing executable directory".into()))?;
            let sibling = parent.join("lumeryn.exe");
            if sibling.is_file() {
                sibling
            } else if parent.file_name().is_some_and(|name| name == "deps") {
                parent
                    .parent()
                    .ok_or_else(|| Error::Invalid("missing Cargo target directory".into()))?
                    .join("lumeryn.exe")
            } else {
                sibling
            }
        };
        if !executable.is_file() {
            return Err(Error::NotFound(format!(
                "project service executable {}",
                executable.display()
            )));
        }
        #[cfg(windows)]
        {
            crate::windows::launch_service(&executable, &self.project)
        }
        #[cfg(not(windows))]
        {
            let _ = executable;
            Err(Error::Unsupported(
                "secure local service transport currently requires Windows".into(),
            ))
        }
    }
    pub async fn call(&self, mut request: Request) -> Result<Value> {
        normalize_paths(&mut request)?;
        #[cfg(windows)]
        {
            let mut pipe = self.open().await?;
            let id = Uuid::new_v4();
            write_frame(
                &mut pipe,
                &Envelope {
                    version: PROTOCOL_VERSION,
                    id,
                    request,
                },
            )
            .await?;
            // Render deadlines are enforced per media worker; watch has its own bounded deadline.
            let response: Response = read_frame(&mut pipe).await?;
            if response.version != PROTOCOL_VERSION || response.id != id {
                return Err(Error::Invalid(
                    "IPC response version/correlation mismatch".into(),
                ));
            }
            response.result.map_err(Into::into)
        }
        #[cfg(not(windows))]
        {
            let _ = request;
            Err(Error::Unsupported(
                "secure local service transport currently requires Windows".into(),
            ))
        }
    }
    pub async fn inspect(&self) -> Result<Project> {
        Ok(serde_json::from_value(self.call(Request::Inspect).await?)?)
    }
    pub async fn history(&self) -> Result<Vec<Value>> {
        Ok(serde_json::from_value(self.call(Request::History).await?)?)
    }
    pub async fn execute(
        &self,
        batch: Batch,
        expected_revision: Option<u64>,
        key: Option<String>,
        dry_run: bool,
        tools: ToolConfig,
    ) -> Result<Value> {
        self.call(Request::Execute {
            batch,
            expected_revision,
            key,
            dry_run,
            tools,
        })
        .await
    }
    pub async fn watch(&self, after_sequence: Option<u64>, wait_ms: u32) -> Result<EventBatch> {
        Ok(serde_json::from_value(
            self.call(Request::Watch {
                after_sequence,
                wait_ms,
            })
            .await?,
        )?)
    }
}

fn normalize_paths(request: &mut Request) -> Result<()> {
    let normalize_tool = |path: &mut PathBuf| -> Result<()> {
        if path.is_relative() && path.components().count() > 1 {
            *path = std::path::absolute(&*path)?;
        }
        Ok(())
    };
    match request {
        Request::Execute { batch, tools, .. } => {
            for operation in &mut batch.operations {
                if let lumeryn_core::Edit::Import { path, .. } = operation {
                    *path = std::path::absolute(&*path)?;
                }
            }
            normalize_tool(&mut tools.ffmpeg)?;
            normalize_tool(&mut tools.ffprobe)?;
        }
        Request::Backup { output } => {
            *output = std::path::absolute(&*output)?;
        }
        Request::Render { output, tools, .. } => {
            *output = std::path::absolute(&*output)?;
            normalize_tool(&mut tools.ffmpeg)?;
            normalize_tool(&mut tools.ffprobe)?;
        }
        _ => {}
    }
    Ok(())
}
