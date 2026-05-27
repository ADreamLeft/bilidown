use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::fs_utils::project_dirs;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthState {
    pub refresh_token: Option<String>,
}

pub fn cookie_path() -> anyhow::Result<PathBuf> {
    Ok(project_dirs()?.config_dir().join("cookies.json"))
}

pub fn auth_state_path() -> anyhow::Result<PathBuf> {
    Ok(project_dirs()?.config_dir().join("auth.toml"))
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
