use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::fs_utils::project_dirs;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    pub output_dir: Option<PathBuf>,
    pub template: Option<String>,
    pub quality: Option<String>,
    pub codec: Option<String>,
    pub audio_quality: Option<String>,
    pub connections: Option<usize>,
    pub retries: Option<usize>,
    pub cover: Option<bool>,
    pub subtitle: Option<bool>,
    pub danmaku: Option<bool>,
    pub skip_archived: Option<bool>,
    pub save_archive: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthState {
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigKey {
    OutputDir,
    Template,
    Quality,
    Codec,
    AudioQuality,
    Connections,
    Retries,
    Cover,
    Subtitle,
    Danmaku,
    SkipArchived,
    SaveArchive,
}

impl std::str::FromStr for ConfigKey {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "output_dir" | "out-dir" => Ok(Self::OutputDir),
            "template" => Ok(Self::Template),
            "quality" => Ok(Self::Quality),
            "codec" => Ok(Self::Codec),
            "audio_quality" | "audio-quality" => Ok(Self::AudioQuality),
            "connections" => Ok(Self::Connections),
            "retries" => Ok(Self::Retries),
            "cover" => Ok(Self::Cover),
            "subtitle" => Ok(Self::Subtitle),
            "danmaku" => Ok(Self::Danmaku),
            "skip_archived" | "skip-archived" => Ok(Self::SkipArchived),
            "save_archive" | "save-archive" => Ok(Self::SaveArchive),
            _ => anyhow::bail!("unknown config key: {s}"),
        }
    }
}

impl AppConfig {
    pub fn set(&mut self, key: ConfigKey, value: &str) -> anyhow::Result<()> {
        match key {
            ConfigKey::OutputDir => self.output_dir = Some(PathBuf::from(value)),
            ConfigKey::Template => self.template = Some(value.to_string()),
            ConfigKey::Quality => self.quality = Some(value.to_string()),
            ConfigKey::Codec => self.codec = Some(value.to_string()),
            ConfigKey::AudioQuality => self.audio_quality = Some(value.to_string()),
            ConfigKey::Connections => self.connections = Some(parse_nonzero(value, "connections")?),
            ConfigKey::Retries => self.retries = Some(parse_nonzero(value, "retries")?),
            ConfigKey::Cover => self.cover = Some(parse_bool(value)?),
            ConfigKey::Subtitle => self.subtitle = Some(parse_bool(value)?),
            ConfigKey::Danmaku => self.danmaku = Some(parse_bool(value)?),
            ConfigKey::SkipArchived => self.skip_archived = Some(parse_bool(value)?),
            ConfigKey::SaveArchive => self.save_archive = Some(parse_bool(value)?),
        }
        Ok(())
    }

    pub fn unset(&mut self, key: ConfigKey) {
        match key {
            ConfigKey::OutputDir => self.output_dir = None,
            ConfigKey::Template => self.template = None,
            ConfigKey::Quality => self.quality = None,
            ConfigKey::Codec => self.codec = None,
            ConfigKey::AudioQuality => self.audio_quality = None,
            ConfigKey::Connections => self.connections = None,
            ConfigKey::Retries => self.retries = None,
            ConfigKey::Cover => self.cover = None,
            ConfigKey::Subtitle => self.subtitle = None,
            ConfigKey::Danmaku => self.danmaku = None,
            ConfigKey::SkipArchived => self.skip_archived = None,
            ConfigKey::SaveArchive => self.save_archive = None,
        }
    }
}

pub fn config_path() -> anyhow::Result<PathBuf> {
    Ok(project_dirs()?.config_dir().join("config.toml"))
}

pub fn cookie_path() -> anyhow::Result<PathBuf> {
    Ok(project_dirs()?.config_dir().join("cookies.json"))
}

pub fn auth_state_path() -> anyhow::Result<PathBuf> {
    Ok(project_dirs()?.config_dir().join("auth.toml"))
}

pub fn read_app_config() -> anyhow::Result<AppConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let text = std::fs::read_to_string(path)?;
    Ok(toml::from_str(&text)?)
}

pub fn write_app_config(cfg: &AppConfig) -> anyhow::Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, toml::to_string_pretty(cfg)?)?;
    Ok(())
}

pub fn read_auth_state() -> anyhow::Result<AuthState> {
    let path = auth_state_path()?;
    if !path.exists() {
        return Ok(AuthState::default());
    }
    let text = std::fs::read_to_string(path)?;
    Ok(toml::from_str(&text)?)
}

pub fn write_auth_state(state: &AuthState) -> anyhow::Result<()> {
    let path = auth_state_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, toml::to_string_pretty(state)?)?;
    Ok(())
}

fn parse_nonzero(value: &str, name: &str) -> anyhow::Result<usize> {
    let value = value.parse::<usize>()?;
    if value == 0 {
        anyhow::bail!("{name} must be greater than 0");
    }
    Ok(value)
}

fn parse_bool(value: &str) -> anyhow::Result<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => anyhow::bail!("invalid bool value: {value}"),
    }
}
