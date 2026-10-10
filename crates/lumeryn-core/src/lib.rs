//! Shared proprietary editing engine. CLI and future Studio use this command layer.
pub mod clipboard;
pub mod error;
pub mod library;
pub mod media;
pub mod model;
pub mod render;
pub mod storage;
pub mod time;

pub use error::{Error, Result};
pub use media::MediaTools;
pub use model::{Batch, Edit, Project};
pub use storage::Store;
