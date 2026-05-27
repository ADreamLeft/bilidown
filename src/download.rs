use std::path::{Path, PathBuf};

use anyhow::Context;
use futures_util::{StreamExt, future::try_join_all};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::{
    StatusCode,
    header::{ACCEPT_ENCODING, ACCEPT_RANGES, CONTENT_LENGTH, RANGE, REFERER, USER_AGENT},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{REFERER as BILI_REFERER, USER_AGENT as BILI_USER_AGENT, client::BiliClient};

#[derive(Debug, Clone, Copy)]
pub struct DownloadConfig {
    pub connections: usize,
    pub retries: usize,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            connections: 8,
            retries: 3,
        }
    }
}

pub async fn download_stream(
    client: &BiliClient,
    url: &str,
    dest: &Path,
    label: &str,
) -> anyhow::Result<()> {
    download_stream_with_urls(
        client,
        &[url.to_string()],
        dest,
        label,
        DownloadConfig {
            connections: 1,
            ..DownloadConfig::default()
        },
    )
    .await
}

pub async fn download_stream_with_urls(
    client: &BiliClient,
    urls: &[String],
    dest: &Path,
    label: &str,
    config: DownloadConfig,
) -> anyhow::Result<()> {
    if dest.exists() {
        return Ok(());
    }

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut last_error = None;
    for url in urls {
        match download_stream_one_url(client, url, dest, label, config).await {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_error = Some(err);
                cleanup_partial(dest).await;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("no download URL provided")))
}

async fn download_stream_one_url(
    client: &BiliClient,
    url: &str,
    dest: &Path,
    label: &str,
    config: DownloadConfig,
) -> anyhow::Result<()> {
    if config.connections > 1
        && let Ok(Some(total)) = probe_range_size(client, url).await
    {
        return parallel_download(client, url, dest, label, total, config).await;
    }
    sequential_download(client, url, dest, label, config).await
}

async fn sequential_download(
    client: &BiliClient,
    url: &str,
    dest: &Path,
    label: &str,
    config: DownloadConfig,
) -> anyhow::Result<()> {
    let tmp = tmp_path(dest);
    let existing = match tokio::fs::metadata(&tmp).await {
        Ok(meta) => meta.len(),
        Err(_) => 0,
    };

    let mut request = client
        .http()
        .get(url)
        .header(USER_AGENT, BILI_USER_AGENT)
        .header(REFERER, BILI_REFERER)
        .header(ACCEPT_ENCODING, "identity");
    if existing > 0 {
        request = request.header(RANGE, format!("bytes={existing}-"));
    }

    let response = send_with_retries(request, config.retries, || format!("download {url}"))
        .await?
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

async fn parallel_download(
    client: &BiliClient,
    url: &str,
    dest: &Path,
    label: &str,
    total: u64,
    config: DownloadConfig,
) -> anyhow::Result<()> {
    let parts_dir = dest.with_extension("parts");
    tokio::fs::create_dir_all(&parts_dir).await?;
    let ranges = split_ranges(total, config.connections);

    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::with_template(
            "{prefix:.bold} {bar:40.cyan/blue} {bytes}/{total_bytes} {bytes_per_sec}",
        )
        .unwrap()
        .progress_chars("=>-"),
    );
    pb.set_prefix(label.to_string());

    let mut tasks = Vec::new();
    for (idx, (start, end)) in ranges.into_iter().enumerate() {
        let path = parts_dir.join(format!("{idx:05}.part"));
        let url = url.to_string();
        let client = client.clone();
        let pb = pb.clone();
        tasks.push(async move {
            download_part(&client, &url, &path, start, end, config.retries, pb).await
        });
    }
    try_join_all(tasks).await?;
    pb.finish_and_clear();

    let tmp = tmp_path(dest);
    let mut out = tokio::fs::File::create(&tmp).await?;
    for idx in 0..config.connections.min(total as usize) {
        let path = parts_dir.join(format!("{idx:05}.part"));
        let mut part = tokio::fs::File::open(&path).await?;
        let mut buf = Vec::new();
        part.read_to_end(&mut buf).await?;
        out.write_all(&buf).await?;
    }
    out.flush().await?;
    tokio::fs::rename(&tmp, dest).await?;
    let _ = tokio::fs::remove_dir_all(&parts_dir).await;
    Ok(())
}

async fn download_part(
    client: &BiliClient,
    url: &str,
    path: &Path,
    start: u64,
    end: u64,
    retries: usize,
    pb: ProgressBar,
) -> anyhow::Result<()> {
    let existing = match tokio::fs::metadata(path).await {
        Ok(meta) => meta.len(),
        Err(_) => 0,
    };
    let expected = end - start + 1;
    if existing == expected {
        pb.inc(existing);
        return Ok(());
    }
    let from = start + existing.min(expected);
    let mut file = if existing > 0 && existing < expected {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(path)
            .await?
    } else {
        tokio::fs::File::create(path).await?
    };
    if existing > 0 && existing < expected {
        pb.inc(existing);
    }

    let request = client
        .http()
        .get(url)
        .header(USER_AGENT, BILI_USER_AGENT)
        .header(REFERER, BILI_REFERER)
        .header(ACCEPT_ENCODING, "identity")
        .header(RANGE, format!("bytes={from}-{end}"));
    let response = send_with_retries(request, retries, || {
        format!("download range {from}-{end} from {url}")
    })
    .await?
    .error_for_status()?;
    anyhow::ensure!(
        response.status() == StatusCode::PARTIAL_CONTENT,
        "server did not return partial content for range request"
    );
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("read response chunk")?;
        file.write_all(&chunk).await?;
        pb.inc(chunk.len() as u64);
    }
    file.flush().await?;
    Ok(())
}

async fn probe_range_size(client: &BiliClient, url: &str) -> anyhow::Result<Option<u64>> {
    let response = client
        .http()
        .get(url)
        .header(USER_AGENT, BILI_USER_AGENT)
        .header(REFERER, BILI_REFERER)
        .header(ACCEPT_ENCODING, "identity")
        .header(RANGE, "bytes=0-0")
        .send()
        .await?;
    if response.status() == StatusCode::PARTIAL_CONTENT {
        let total = response
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit('/').next())
            .and_then(|v| v.parse::<u64>().ok());
        return Ok(total);
    }
    if response
        .headers()
        .get(ACCEPT_RANGES)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("bytes"))
    {
        return Ok(response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok()));
    }
    Ok(None)
}

async fn send_with_retries(
    request: reqwest::RequestBuilder,
    retries: usize,
    label: impl Fn() -> String,
) -> anyhow::Result<reqwest::Response> {
    let attempts = retries.max(1);
    let mut last_error = None;
    for attempt in 0..attempts {
        let req = request
            .try_clone()
            .context("request cannot be cloned for retry")?;
        match req.send().await {
            Ok(response) => return Ok(response),
            Err(err) => {
                last_error = Some(err);
                if attempt + 1 < attempts {
                    tokio::time::sleep(std::time::Duration::from_millis(
                        250 * (attempt as u64 + 1),
                    ))
                    .await;
                }
            }
        }
    }
    Err(anyhow::anyhow!(
        "{}: {}",
        label(),
        last_error
            .map(|e| e.to_string())
            .unwrap_or_else(|| "unknown error".to_string())
    ))
}

fn split_ranges(total: u64, connections: usize) -> Vec<(u64, u64)> {
    let connections = connections.max(1).min(total.max(1) as usize);
    let base = total / connections as u64;
    let mut rem = total % connections as u64;
    let mut start = 0;
    let mut out = Vec::new();
    for _ in 0..connections {
        let mut len = base;
        if rem > 0 {
            len += 1;
            rem -= 1;
        }
        let end = start + len - 1;
        out.push((start, end));
        start = end + 1;
    }
    out
}

async fn cleanup_partial(dest: &Path) {
    let _ = tokio::fs::remove_file(tmp_path(dest)).await;
    let _ = tokio::fs::remove_dir_all(dest.with_extension("parts")).await;
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
