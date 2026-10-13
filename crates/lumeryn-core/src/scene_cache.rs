//! Process-local static scene reuse; outstanding shared buffers are not retained-cache bytes.
use crate::scene_text::{FontRef, SceneDocument, SceneError, SceneRenderer};
use sha2::{Digest, Sha256};
use std::{collections::VecDeque, sync::Arc};

const KEY_DOMAIN: &[u8] = b"lumeryn.static-scene.rgba8.straight.encoded-sdr.v1\0";
pub const DEFAULT_CACHE_BYTES: usize = 64 * 1024 * 1024;
pub const DEFAULT_CACHE_ENTRIES: usize = 16;

#[derive(Clone, Debug)]
pub struct SceneRaster {
    pub pixels: Arc<[u8]>,
    pub width: u32,
    pub height: u32,
    pub key: [u8; 32],
    pub cache_hit: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub retained_bytes: usize,
    pub entries: usize,
}

struct Entry {
    key: [u8; 32],
    pixels: Arc<[u8]>,
}

/// Own one verified-font renderer on its worker. Zero limits disable retention.
pub struct StaticSceneCache {
    renderer: SceneRenderer,
    font: FontRef,
    max_bytes: usize,
    max_entries: usize,
    entries: VecDeque<Entry>,
    stats: CacheStats,
}

impl StaticSceneCache {
    pub fn new(
        font: FontRef,
        bytes: Vec<u8>,
        max_bytes: usize,
        max_entries: usize,
    ) -> Result<Self, SceneError> {
        let renderer = SceneRenderer::new(font.clone(), bytes)?;
        if max_bytes > DEFAULT_CACHE_BYTES || max_entries > DEFAULT_CACHE_ENTRIES {
            return Err(SceneError::Limit("static scene cache policy"));
        }
        Ok(Self {
            renderer,
            font,
            max_bytes,
            max_entries,
            entries: VecDeque::new(),
            stats: CacheStats::default(),
        })
    }

    pub fn stats(&self) -> CacheStats {
        self.stats
    }

    pub fn render_cached(
        &mut self,
        document: &SceneDocument,
        width: u32,
        height: u32,
    ) -> Result<SceneRaster, SceneError> {
        document.validate()?;
        if !(1..=3840).contains(&width) || !(1..=3840).contains(&height) {
            return Err(SceneError::Limit("canvas dimensions"));
        }
        if u64::from(document.width) * u64::from(height)
            != u64::from(document.height) * u64::from(width)
        {
            return Err(SceneError::Invalid("output aspect ratio"));
        }
        let expected_bytes = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(SceneError::Limit("frame bytes"))?;
        let encoded = serde_json::to_vec(&(document, &self.font, width, height))
            .map_err(|_| SceneError::Invalid("scene cache key serialization"))?;
        let mut digest = Sha256::new();
        digest.update(KEY_DOMAIN);
        digest.update(encoded);
        let key: [u8; 32] = digest.finalize().into();
        if let Some(index) = self.entries.iter().position(|entry| entry.key == key) {
            let entry = self
                .entries
                .remove(index)
                .ok_or(SceneError::Invalid("scene cache entry"))?;
            let pixels = Arc::clone(&entry.pixels);
            self.entries.push_back(entry);
            self.stats.hits = self.stats.hits.saturating_add(1);
            return Ok(SceneRaster {
                pixels,
                width,
                height,
                key,
                cache_hit: true,
            });
        }
        self.stats.misses = self.stats.misses.saturating_add(1);
        let pixels: Arc<[u8]> = self.renderer.render(document, width, height)?.into();
        if pixels.len() != expected_bytes {
            return Err(SceneError::Invalid("scene raster byte count"));
        }
        if self.max_entries > 0 && pixels.len() <= self.max_bytes {
            while self.entries.len() >= self.max_entries
                || self.stats.retained_bytes > self.max_bytes - pixels.len()
            {
                let Some(entry) = self.entries.pop_front() else {
                    return Err(SceneError::Invalid("scene cache accounting"));
                };
                self.stats.retained_bytes -= entry.pixels.len();
                self.stats.evictions = self.stats.evictions.saturating_add(1);
            }
            self.stats.retained_bytes += pixels.len();
            self.entries.push_back(Entry {
                key,
                pixels: Arc::clone(&pixels),
            });
            self.stats.entries = self.entries.len();
        }
        Ok(SceneRaster {
            pixels,
            width,
            height,
            key,
            cache_hit: false,
        })
    }
}
