//! Read-only, offline asset catalogue shared by native Studio and external CLI agents.
//! Imported files remain ordinary immutable project assets; this module is not an editor writer.
use crate::{Error, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::File,
    io::Read,
    path::{Component, Path, PathBuf},
};

const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ASSET_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ENTRIES: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Audio,
    Image,
    Svg,
    Font,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssetEntry {
    pub id: String,
    pub name: String,
    pub kind: AssetKind,
    pub path: String,
    pub pack_id: String,
    pub category: String,
    pub tags: Vec<String>,
    pub source_url: String,
    pub license_id: String,
    pub license_path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub attribution: String,
    pub duration_ms: Option<u64>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    entries: Vec<AssetEntry>,
}

#[derive(Debug)]
pub struct Catalog {
    root: PathBuf,
    entries: Vec<AssetEntry>,
}

#[derive(Debug, Serialize)]
pub struct LibraryCheck {
    pub assets: usize,
    pub bytes: u64,
    pub hashes: &'static str,
    pub notices: &'static str,
}

impl Catalog {
    /// Loads bounded metadata only. File bytes are verified explicitly before use.
    pub fn load(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().canonicalize()?;
        let mut input = File::open(root.join("catalog.json"))?.take(MAX_MANIFEST_BYTES + 1);
        let mut bytes = Vec::new();
        input.read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(Error::Invalid("asset catalogue exceeds 4 MiB".into()));
        }
        let manifest: Manifest = serde_json::from_slice(&bytes)?;
        if manifest.schema_version != 1 || manifest.entries.len() > MAX_ENTRIES {
            return Err(Error::Unsupported("asset catalogue schema or size".into()));
        }
        let mut ids = HashSet::new();
        for entry in &manifest.entries {
            if !ids.insert(&entry.id) {
                return Err(Error::Invalid("duplicate asset ID".into()));
            }
            validate_entry(entry)?;
        }
        Ok(Self {
            root,
            entries: manifest.entries,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn entries(&self) -> &[AssetEntry] {
        &self.entries
    }

    pub fn get(&self, id: &str) -> Result<&AssetEntry> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| Error::NotFound(format!("library asset {id}")))
    }

    /// Bounded metadata search; no decoding, remote requests or project mutations.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<&AssetEntry>> {
        if query.len() > 256 || !(1..=128).contains(&limit) {
            return Err(Error::Invalid(
                "library query must be <=256 bytes; limit 1..128".into(),
            ));
        }
        let query = query.to_lowercase();
        Ok(self
            .entries
            .iter()
            .filter(|entry| {
                [&entry.id, &entry.name, &entry.category, &entry.pack_id]
                    .into_iter()
                    .chain(entry.tags.iter())
                    .any(|text| text.to_lowercase().contains(&query))
            })
            .take(limit)
            .collect())
    }

    /// Resolve, contain and stream-verify a selected asset before import/preview.
    pub fn verified_path(&self, id: &str) -> Result<PathBuf> {
        let entry = self.get(id)?;
        let path = contained_file(&self.root, &entry.path)?;
        let notice = contained_file(&self.root, &entry.license_path)?;
        let notice_size = notice.metadata()?.len();
        if notice_size == 0 || notice_size > 1024 * 1024 {
            return Err(Error::Invalid(
                "missing or oversized asset licence notice".into(),
            ));
        }
        let mut input = File::open(&path)?;
        if input.metadata()?.len() != entry.size_bytes {
            return Err(Error::Invalid(format!("asset size changed: {id}")));
        }
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        let mut size = 0_u64;
        loop {
            let read = input.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            size += read as u64;
            if size > MAX_ASSET_BYTES {
                return Err(Error::Invalid("asset exceeds verification bound".into()));
            }
            hasher.update(&buffer[..read]);
        }
        if size != entry.size_bytes
            || input.metadata()?.len() != size
            || format!("{:x}", hasher.finalize()) != entry.sha256
        {
            return Err(Error::Invalid(format!("asset checksum changed: {id}")));
        }
        Ok(path)
    }

    pub fn check(&self) -> Result<LibraryCheck> {
        let mut bytes = 0_u64;
        for entry in &self.entries {
            self.verified_path(&entry.id)?;
            bytes += entry.size_bytes;
        }
        Ok(LibraryCheck {
            assets: self.entries.len(),
            bytes,
            hashes: "verified_sha256",
            notices: "present; redistribution rights reviewed separately",
        })
    }
}

fn validate_entry(entry: &AssetEntry) -> Result<()> {
    for text in [
        &entry.id,
        &entry.name,
        &entry.pack_id,
        &entry.category,
        &entry.license_id,
    ] {
        if text.trim().is_empty() || text.len() > 256 || text.chars().any(char::is_control) {
            return Err(Error::Invalid("invalid asset metadata text".into()));
        }
    }
    relative_path(&entry.path)?;
    relative_path(&entry.license_path)?;
    if entry.sha256.len() != 64
        || !entry
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || entry.size_bytes == 0
        || entry.size_bytes > MAX_ASSET_BYTES
        || entry.tags.len() > 64
        || entry
            .tags
            .iter()
            .any(|tag| tag.len() > 128 || tag.chars().any(char::is_control))
        || !entry.source_url.starts_with("https://")
        || entry.source_url.len() > 2048
        || entry.attribution.len() > 2048
    {
        return Err(Error::Invalid(
            "invalid asset digest, size or provenance".into(),
        ));
    }
    Ok(())
}

fn relative_path(text: &str) -> Result<&Path> {
    let path = Path::new(text);
    if text.is_empty()
        || text.len() > 1024
        || text.contains(['\\', ':'])
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || text
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err(Error::Invalid(
            "library paths must be contained relative paths".into(),
        ));
    }
    Ok(path)
}

fn contained_file(root: &Path, text: &str) -> Result<PathBuf> {
    let path = root.join(relative_path(text)?).canonicalize()?;
    if !path.starts_with(root) || !path.is_file() {
        return Err(Error::Invalid(
            "asset path escapes the library or is not a file".into(),
        ));
    }
    Ok(path)
}

pub fn default_root() -> Result<PathBuf> {
    if let Some(root) = std::env::var_os("LUMERYN_ASSET_LIBRARY") {
        if root.is_empty() {
            return Err(Error::Invalid("LUMERYN_ASSET_LIBRARY is empty".into()));
        }
        return Ok(root.into());
    }
    let packaged = std::env::current_exe()?
        .parent()
        .ok_or_else(|| Error::Invalid("executable has no parent directory".into()))?
        .join("assets/library");
    if packaged.join("catalog.json").is_file() {
        return Ok(packaged);
    }
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/library");
    if workspace.join("catalog.json").is_file() {
        return Ok(workspace);
    }
    Err(Error::NotFound(
        "asset library; supply --library or LUMERYN_ASSET_LIBRARY".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Result<(tempfile::TempDir, AssetEntry)> {
        let root = tempfile::tempdir()?;
        std::fs::write(root.path().join("cue.wav"), b"test asset")?;
        std::fs::write(root.path().join("LICENSE.txt"), b"CC0 test notice")?;
        let entry = AssetEntry {
            id: "test.cue".into(),
            name: "Reveal cue".into(),
            kind: AssetKind::Audio,
            path: "cue.wav".into(),
            pack_id: "test".into(),
            category: "sound effects".into(),
            tags: vec!["reveal".into()],
            source_url: "https://example.test/cue".into(),
            license_id: "CC0-1.0".into(),
            license_path: "LICENSE.txt".into(),
            sha256: format!("{:x}", Sha256::digest(b"test asset")),
            size_bytes: 10,
            attribution: "test fixture".into(),
            duration_ms: None,
            sample_rate: None,
            channels: None,
            width: None,
            height: None,
        };
        write_manifest(root.path(), std::slice::from_ref(&entry))?;
        Ok((root, entry))
    }

    fn write_manifest(root: &Path, entries: &[AssetEntry]) -> Result<()> {
        std::fs::write(
            root.join("catalog.json"),
            serde_json::to_vec(&serde_json::json!({"schema_version":1,"entries":entries}))?,
        )?;
        Ok(())
    }

    #[test]
    fn search_integrity_and_same_size_replacement() -> Result<()> {
        let (root, _) = fixture()?;
        let catalog = Catalog::load(root.path())?;
        assert_eq!(catalog.search("REVEAL", 10)?.len(), 1);
        assert_eq!(catalog.search("absent", 10)?.len(), 0);
        assert_eq!(catalog.check()?.assets, 1);
        assert!(catalog.search("", 0).is_err());
        std::fs::write(root.path().join("cue.wav"), b"bad asset!")?;
        assert!(catalog.verified_path("test.cue").is_err());
        Ok(())
    }

    #[test]
    fn metadata_rejects_paths_duplicate_ids_and_unknown_fields() -> Result<()> {
        let (root, mut entry) = fixture()?;
        for path in [
            "../cue.wav",
            "/cue.wav",
            "C:/cue.wav",
            "x\\cue.wav",
            "x//cue.wav",
            "./cue.wav",
        ] {
            entry.path = path.into();
            write_manifest(root.path(), std::slice::from_ref(&entry))?;
            assert!(Catalog::load(root.path()).is_err(), "{path}");
        }
        entry.path = "cue.wav".into();
        write_manifest(root.path(), &[entry.clone(), entry])?;
        assert!(Catalog::load(root.path()).is_err());
        std::fs::write(
            root.path().join("catalog.json"),
            br#"{"schema_version":1,"entries":[],"execute":"bad"}"#,
        )?;
        assert!(Catalog::load(root.path()).is_err());
        Ok(())
    }
}
