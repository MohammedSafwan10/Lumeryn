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
pub mod animation;
pub mod animation_backend;
pub mod audio_plan;
pub mod scene_cache;
pub mod scene_text;
pub mod templates;
pub mod time_effects;
pub mod transition_backend;
pub mod visual_backend;
pub mod visual_effects;
pub mod visual_plan;
pub mod visual_transform;
