use std::path::{Path, PathBuf};

use anyhow::Context;
use tokio::process::Command;

pub async fn mux_to_mp4(
    ffmpeg_path: Option<&Path>,
    video_path: &Path,
    audio_path: &Path,
    output_path: &Path,
) -> anyhow::Result<()> {
    let ffmpeg = resolve_ffmpeg(ffmpeg_path)?;
    if let Some(parent) = output_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let status = Command::new(&ffmpeg)
        .arg("-y")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("warning")
        .arg("-i")
        .arg(video_path)
        .arg("-i")
        .arg(audio_path)
        .arg("-c")
        .arg("copy")
        .arg("-movflags")
        .arg("faststart")
        .arg(output_path)
        .status()
        .await
        .with_context(|| format!("execute ffmpeg at {}", ffmpeg.display()))?;

    if !status.success() {
        anyhow::bail!("ffmpeg failed with status {status}");
    }
    Ok(())
}

fn resolve_ffmpeg(path: Option<&Path>) -> anyhow::Result<PathBuf> {
    if let Some(path) = path {
        if path.exists() {
            return Ok(path.to_path_buf());
        }
        anyhow::bail!("ffmpeg path does not exist: {}", path.display());
    }
    which::which("ffmpeg").context("ffmpeg not found; install ffmpeg or pass --ffmpeg-path")
}
