use std::path::{Path, PathBuf};

use anyhow::Context;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::{
    StatusCode,
    header::{RANGE, REFERER, USER_AGENT},
};
use tokio::io::AsyncWriteExt;

use crate::{REFERER as BILI_REFERER, USER_AGENT as BILI_USER_AGENT, client::BiliClient};

pub async fn download_stream(
    client: &BiliClient,
    url: &str,
    dest: &Path,
    label: &str,
) -> anyhow::Result<()> {
    if dest.exists() {
        return Ok(());
    }

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let tmp = tmp_path(dest);
    let existing = match tokio::fs::metadata(&tmp).await {
        Ok(meta) => meta.len(),
        Err(_) => 0,
    };

    let mut request = client
        .http()
        .get(url)
        .header(USER_AGENT, BILI_USER_AGENT)
        .header(REFERER, BILI_REFERER);
    if existing > 0 {
        request = request.header(RANGE, format!("bytes={existing}-"));
    }

    let response = request
        .send()
        .await
        .with_context(|| format!("download {url}"))?
        .error_for_status()
        .with_context(|| format!("download {url} returned error status"))?;

    let append = response.status() == StatusCode::PARTIAL_CONTENT && existing > 0;
    let initial_pos = if append { existing } else { 0 };
    let total = response.content_length().map(|len| len + initial_pos);

    let mut file = if append {
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&tmp)
            .await?
    } else {
        tokio::fs::File::create(&tmp).await?
    };

    let pb = match total {
        Some(total) => ProgressBar::new(total),
        None => ProgressBar::new_spinner(),
    };
    pb.set_style(
        ProgressStyle::with_template(
            "{prefix:.bold} {bar:40.cyan/blue} {bytes}/{total_bytes} {bytes_per_sec}",
        )
        .unwrap()
        .progress_chars("=>-"),
    );
    pb.set_prefix(label.to_string());
    pb.set_position(initial_pos);

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("read response chunk")?;
        file.write_all(&chunk).await?;
        pb.inc(chunk.len() as u64);
    }
    file.flush().await?;
    pb.finish_and_clear();

    tokio::fs::rename(&tmp, dest)
        .await
        .with_context(|| format!("rename {} to {}", tmp.display(), dest.display()))?;
    Ok(())
}

pub fn sidecar_path(output: &Path, suffix: &str) -> PathBuf {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let stem = output
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    parent.join(format!("{stem}.{suffix}"))
}

fn tmp_path(dest: &Path) -> PathBuf {
    let ext = dest
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!("{e}.tmp"))
        .unwrap_or_else(|| "tmp".to_string());
    dest.with_extension(ext)
}
