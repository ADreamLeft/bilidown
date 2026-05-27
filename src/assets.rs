use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Deserialize;

use crate::{
    client::BiliClient,
    download::{DownloadConfig, download_stream_with_urls},
    fs_utils::safe_path_component,
};

#[derive(Debug, Clone, Default)]
pub struct AssetOptions {
    pub cover: bool,
    pub subtitle: bool,
    pub danmaku: bool,
    pub embed_subtitle: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerV2Response {
    pub code: i64,
    pub message: Option<String>,
    pub data: Option<PlayerV2Data>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerV2Data {
    pub subtitle: Option<SubtitleInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubtitleInfo {
    #[serde(default)]
    pub subtitles: Vec<SubtitleItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubtitleItem {
    pub lan: String,
    pub lan_doc: String,
    #[serde(alias = "subtitle_url")]
    pub subtitle_url: String,
}

#[derive(Debug, Deserialize)]
struct BiliSubtitle {
    body: Vec<BiliSubtitleLine>,
}

#[derive(Debug, Deserialize)]
struct BiliSubtitleLine {
    from: f64,
    to: f64,
    content: String,
}

pub async fn download_cover(
    client: &BiliClient,
    cover_url: &str,
    output: &Path,
    config: DownloadConfig,
) -> anyhow::Result<Option<PathBuf>> {
    if cover_url.is_empty() {
        return Ok(None);
    }
    let ext = detect_image_extension(cover_url);
    let path = output.with_extension(ext);
    download_stream_with_urls(client, &[cover_url.to_string()], &path, "cover", config).await?;
    Ok(Some(path))
}

pub async fn fetch_subtitles(
    client: &BiliClient,
    aid: u64,
    cid: u64,
) -> anyhow::Result<Vec<SubtitleItem>> {
    let url = format!("https://api.bilibili.com/x/player/v2?aid={aid}&cid={cid}");
    let resp: PlayerV2Response = client.get_json(&url).await?;
    if resp.code != 0 {
        anyhow::bail!(
            "player v2 API failed: code={}, message={}",
            resp.code,
            resp.message.unwrap_or_default()
        );
    }
    Ok(resp
        .data
        .and_then(|data| data.subtitle)
        .map(|subtitle| subtitle.subtitles)
        .unwrap_or_default())
}

pub async fn download_subtitles(
    client: &BiliClient,
    subtitles: &[SubtitleItem],
    output: &Path,
) -> anyhow::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for sub in subtitles {
        let url = normalize_subtitle_url(&sub.subtitle_url);
        let json = client.get_text(&url).await?;
        let srt = bili_subtitle_json_to_srt(&json)?;
        let stem = output
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("subtitle");
        let path = output.with_file_name(format!(
            "{}.{}.srt",
            stem,
            safe_path_component(if sub.lan_doc.is_empty() {
                &sub.lan
            } else {
                &sub.lan_doc
            })
        ));
        tokio::fs::write(&path, srt).await?;
        paths.push(path);
    }
    Ok(paths)
}

pub async fn download_danmaku(
    client: &BiliClient,
    cid: u64,
    output: &Path,
) -> anyhow::Result<PathBuf> {
    let url = format!("https://api.bilibili.com/x/v1/dm/list.so?oid={cid}");
    let text = client.get_text(&url).await?;
    let path = output.with_extension("danmaku.xml");
    tokio::fs::write(&path, text).await?;
    Ok(path)
}

pub fn bili_subtitle_json_to_srt(text: &str) -> anyhow::Result<String> {
    let sub: BiliSubtitle = serde_json::from_str(text).context("parse bilibili subtitle JSON")?;
    let mut out = String::new();
    for (idx, line) in sub.body.into_iter().enumerate() {
        out.push_str(&(idx + 1).to_string());
        out.push('\n');
        out.push_str(&format_time(line.from));
        out.push_str(" --> ");
        out.push_str(&format_time(line.to));
        out.push('\n');
        out.push_str(&line.content.replace(['\r', '\n'], " "));
        out.push_str("\n\n");
    }
    Ok(out)
}

pub fn detect_image_extension(raw_url: &str) -> &'static str {
    let path = raw_url.split('?').next().unwrap_or(raw_url);
    let path = path.split('@').next().unwrap_or(path);
    let ext = path
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "png",
        "webp" => "webp",
        "gif" => "gif",
        "jpeg" => "jpg",
        "jpg" => "jpg",
        _ => "jpg",
    }
}

fn normalize_subtitle_url(raw: &str) -> String {
    if raw.starts_with("//") {
        format!("https:{raw}")
    } else {
        raw.to_string()
    }
}

fn format_time(seconds: f64) -> String {
    let millis = (seconds * 1000.0).round().max(0.0) as u64;
    let hours = millis / 3_600_000;
    let minutes = (millis % 3_600_000) / 60_000;
    let seconds = (millis % 60_000) / 1000;
    let millis = millis % 1000;
    format!("{hours:02}:{minutes:02}:{seconds:02},{millis:03}")
}
