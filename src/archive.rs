use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fs_utils::project_dirs;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ArchiveMode {
    #[default]
    Both,
    Audio,
    Video,
}

impl std::fmt::Display for ArchiveMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Both => f.write_str("both"),
            Self::Audio => f.write_str("audio"),
            Self::Video => f.write_str("video"),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Archive {
    #[serde(default)]
    pub entries: Vec<ArchiveEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub aid: u64,
    pub cid: u64,
    #[serde(default)]
    pub mode: ArchiveMode,
    pub quality: String,
    pub codec: String,
    pub audio: String,
    pub output: String,
    pub completed_at: i64,
}

impl Archive {
    pub fn contains(&self, aid: u64, cid: u64, mode: ArchiveMode) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.aid == aid && entry.cid == cid && entry.mode == mode)
    }

    pub fn add(&mut self, entry: ArchiveEntry) {
        self.entries.retain(|old| {
            !(old.aid == entry.aid && old.cid == entry.cid && old.mode == entry.mode)
        });
        self.entries.push(entry);
    }
}

pub fn default_archive_path() -> anyhow::Result<PathBuf> {
    Ok(project_dirs()?.data_dir().join("archive.toml"))
}

pub fn read_archive(path: impl AsRef<Path>) -> anyhow::Result<Archive> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(Archive::default());
    }
    let text = std::fs::read_to_string(path)?;
    Ok(toml::from_str(&text)?)
}

pub fn write_archive(path: impl AsRef<Path>, archive: &Archive) -> anyhow::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, toml::to_string_pretty(archive)?)?;
    Ok(())
}
