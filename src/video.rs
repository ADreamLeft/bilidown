use std::collections::BTreeMap;

use anyhow::Context;
use serde::Deserialize;

use crate::{client::BiliClient, input::VideoInput, wbi};

#[derive(Debug, Clone)]
pub struct VideoInfo {
    pub aid: u64,
    pub bvid: String,
    pub title: String,
    pub description: String,
    pub cover_url: String,
    pub pub_time: i64,
    pub owner_name: String,
    pub owner_mid: u64,
    pub pages: Vec<VideoPage>,
}

#[derive(Debug, Clone)]
pub struct VideoPage {
    pub index: usize,
    pub cid: u64,
    pub title: String,
    pub duration: u64,
    pub resolution: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoTrack {
    pub quality_id: u32,
    pub quality_name: String,
    pub base_url: String,
    pub backup_urls: Vec<String>,
    pub codec_name: String,
    pub codec_id: Option<u32>,
    pub bandwidth: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame_rate: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioTrack {
    pub id: u32,
    pub base_url: String,
    pub backup_urls: Vec<String>,
    pub codec_name: String,
    pub bandwidth: u64,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPlay {
    pub duration: Option<u64>,
    pub video_tracks: Vec<VideoTrack>,
    pub audio_tracks: Vec<AudioTrack>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityPreference {
    Best,
    Id(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioQualityPreference {
    Best,
    Id(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodecPreference(Vec<VideoCodec>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VideoCodec {
    Av1,
    Hevc,
    Avc,
    Other,
}

impl Default for CodecPreference {
    fn default() -> Self {
        Self(vec![VideoCodec::Av1, VideoCodec::Hevc, VideoCodec::Avc])
    }
}

impl CodecPreference {
    pub fn parse(input: &str) -> anyhow::Result<Self> {
        let mut codecs = Vec::new();
        for raw in input.split(',') {
            let codec = match raw.trim().to_ascii_lowercase().as_str() {
                "" => continue,
                "av1" => VideoCodec::Av1,
                "hevc" | "h265" | "h.265" => VideoCodec::Hevc,
                "avc" | "h264" | "h.264" => VideoCodec::Avc,
                other => anyhow::bail!("unsupported codec preference: {other}"),
            };
            if !codecs.contains(&codec) {
                codecs.push(codec);
            }
        }
        if codecs.is_empty() {
            return Ok(Self::default());
        }
        Ok(Self(codecs))
    }

    fn rank(&self, track: &VideoTrack) -> usize {
        let codec = codec_from_track(track);
        self.0
            .iter()
            .position(|c| *c == codec)
            .unwrap_or(usize::MAX)
    }
}

impl QualityPreference {
    pub fn parse(input: &str) -> anyhow::Result<Self> {
        if input.trim().eq_ignore_ascii_case("best") {
            return Ok(Self::Best);
        }
        Ok(Self::Id(
            input
                .trim()
                .parse::<u32>()
                .with_context(|| format!("invalid quality id: {input}"))?,
        ))
    }
}

impl AudioQualityPreference {
    pub fn parse(input: &str) -> anyhow::Result<Self> {
        if input.trim().eq_ignore_ascii_case("best") {
            return Ok(Self::Best);
        }
        Ok(Self::Id(input.trim().parse::<u32>().with_context(
            || format!("invalid audio quality id: {input}"),
        )?))
    }
}

impl ParsedPlay {
    pub fn select_video(
        &self,
        quality: QualityPreference,
        codec_preference: &CodecPreference,
    ) -> anyhow::Result<VideoTrack> {
        let mut candidates = self.video_tracks.clone();
        if let QualityPreference::Id(id) = quality {
            candidates.retain(|track| track.quality_id == id);
        }
        candidates.sort_by(|a, b| {
            b.quality_id
                .cmp(&a.quality_id)
                .then(codec_preference.rank(a).cmp(&codec_preference.rank(b)))
                .then(b.bandwidth.cmp(&a.bandwidth))
        });
        candidates
            .into_iter()
            .next()
            .context("no matching video stream")
    }

    pub fn select_audio(&self, quality: AudioQualityPreference) -> anyhow::Result<AudioTrack> {
        let mut candidates = self.audio_tracks.clone();
        if let AudioQualityPreference::Id(id) = quality {
            candidates.retain(|track| track.id == id);
        }
        candidates.sort_by(|a, b| b.bandwidth.cmp(&a.bandwidth).then(b.id.cmp(&a.id)));
        candidates
            .into_iter()
            .next()
            .context("no matching audio stream")
    }
}

pub async fn fetch_video_info(
    client: &BiliClient,
    input: &VideoInput,
) -> anyhow::Result<VideoInfo> {
    let url = match input {
        VideoInput::Aid(aid) => {
            format!("https://api.bilibili.com/x/web-interface/view?aid={aid}")
        }
        VideoInput::Bvid(bvid) => {
            format!("https://api.bilibili.com/x/web-interface/view?bvid={bvid}")
        }
    };
    let resp: ViewResponse = client.get_json(&url).await?;
    if resp.code != 0 {
        anyhow::bail!(
            "video info API failed: code={}, message={}",
            resp.code,
            resp.message.unwrap_or_default()
        );
    }
    let data = resp.data.context("video info API returned no data")?;
    Ok(data.into())
}

pub async fn fetch_play_info(
    client: &BiliClient,
    aid: u64,
    cid: u64,
    quality: QualityPreference,
) -> anyhow::Result<ParsedPlay> {
    let wbi_key = client.fetch_wbi_key().await?;
    let mut params = BTreeMap::new();
    params.insert("avid".to_string(), aid.to_string());
    params.insert("cid".to_string(), cid.to_string());
    params.insert("fnval".to_string(), "4048".to_string());
    params.insert("fnver".to_string(), "0".to_string());
    params.insert("fourk".to_string(), "1".to_string());
    params.insert("otype".to_string(), "json".to_string());
    params.insert(
        "qn".to_string(),
        match quality {
            QualityPreference::Best => "127".to_string(),
            QualityPreference::Id(id) => id.to_string(),
        },
    );
    let query = wbi::sign_params(params, &wbi_key).to_query_string();
    let url = format!("https://api.bilibili.com/x/player/wbi/playurl?{query}");
    let text = client.get_text(&url).await?;
    parse_play_response(&text)
}

pub fn parse_play_response(text: &str) -> anyhow::Result<ParsedPlay> {
    let resp: PlayResponse = serde_json::from_str(text).context("parse playurl response")?;
    if resp.code != 0 {
        anyhow::bail!(
            "playurl API failed: code={}, message={}",
            resp.code,
            resp.message.unwrap_or_default()
        );
    }
    let data = resp
        .data
        .or(resp.result)
        .context("playurl API returned no data")?;
    let dash = data
        .dash
        .context("playurl response does not contain DASH")?;

    let mut audio = dash.audio.unwrap_or_default();
    if let Some(dolby) = dash.dolby.and_then(|d| d.audio) {
        audio.extend(dolby);
    }
    if let Some(flac) = dash.flac.and_then(|f| f.audio) {
        audio.push(flac);
    }

    Ok(ParsedPlay {
        duration: dash.duration,
        video_tracks: dash
            .video
            .iter()
            .map(parse_video_track)
            .collect::<anyhow::Result<Vec<_>>>()?,
        audio_tracks: audio
            .iter()
            .map(parse_audio_track)
            .collect::<anyhow::Result<Vec<_>>>()?,
    })
}

pub fn quality_name(id: u32) -> &'static str {
    match id {
        127 => "8K 超高清",
        126 => "杜比视界",
        125 => "HDR 真彩",
        120 => "4K 超清",
        116 => "1080P 高帧率",
        112 => "1080P 高码率",
        80 => "1080P 高清",
        74 => "720P 高帧率",
        64 | 48 => "720P 高清",
        32 => "480P 清晰",
        16 => "360P 流畅",
        6 => "240P 流畅",
        5 => "144P 流畅",
        _ => "UNKNOWN",
    }
}

fn codec_from_track(track: &VideoTrack) -> VideoCodec {
    match track.codec_id {
        Some(13) => VideoCodec::Av1,
        Some(12) => VideoCodec::Hevc,
        Some(7) => VideoCodec::Avc,
        _ => {
            let codecs = track.codec_name.to_ascii_lowercase();
            if codecs.contains("av01") || codecs.contains("av1") {
                VideoCodec::Av1
            } else if codecs.contains("hev") || codecs.contains("hvc") {
                VideoCodec::Hevc
            } else if codecs.contains("avc") {
                VideoCodec::Avc
            } else {
                VideoCodec::Other
            }
        }
    }
}

fn video_codec_name(codecid: Option<u32>, codecs: &str) -> String {
    match codecid {
        Some(13) => "AV1".to_string(),
        Some(12) => "HEVC".to_string(),
        Some(7) => "AVC".to_string(),
        _ => codecs.to_string(),
    }
}

fn audio_codec_name(codecs: &str) -> String {
    match codecs {
        "mp4a.40.2" | "mp4a.40.5" => "M4A".to_string(),
        "ec-3" => "E-AC-3".to_string(),
        "fLaC" => "FLAC".to_string(),
        other => other.to_string(),
    }
}

#[derive(Debug, Deserialize)]
struct ApiOwner {
    mid: u64,
    name: String,
}

#[derive(Debug, Deserialize)]
struct ViewResponse {
    code: i64,
    message: Option<String>,
    data: Option<ApiVideoInfo>,
}

#[derive(Debug, Deserialize)]
struct ApiVideoInfo {
    aid: u64,
    bvid: String,
    title: String,
    #[serde(default)]
    desc: String,
    #[serde(default)]
    pic: String,
    #[serde(default)]
    pubdate: i64,
    owner: ApiOwner,
    #[serde(default)]
    pages: Vec<ApiPage>,
}

#[derive(Debug, Deserialize)]
struct ApiPage {
    page: usize,
    cid: u64,
    part: String,
    #[serde(default)]
    duration: u64,
    #[serde(default)]
    dimension: Option<ApiDimension>,
}

#[derive(Debug, Deserialize)]
struct ApiDimension {
    width: u32,
    height: u32,
}

impl From<ApiVideoInfo> for VideoInfo {
    fn from(value: ApiVideoInfo) -> Self {
        let pages = value.pages.into_iter().map(Into::into).collect();
        Self {
            aid: value.aid,
            bvid: value.bvid,
            title: value.title,
            description: value.desc,
            cover_url: value.pic,
            pub_time: value.pubdate,
            owner_name: value.owner.name,
            owner_mid: value.owner.mid,
            pages,
        }
    }
}

impl From<ApiPage> for VideoPage {
    fn from(value: ApiPage) -> Self {
        Self {
            index: value.page,
            cid: value.cid,
            title: value.part,
            duration: value.duration,
            resolution: value.dimension.map(|d| format!("{}x{}", d.width, d.height)),
        }
    }
}

#[derive(Debug, Deserialize)]
struct PlayResponse {
    code: i64,
    message: Option<String>,
    data: Option<PlayData>,
    result: Option<PlayData>,
}

#[derive(Debug, Deserialize)]
struct PlayData {
    dash: Option<DashData>,
}

#[derive(Debug, Deserialize)]
struct DashData {
    duration: Option<u64>,
    #[serde(default)]
    video: Vec<serde_json::Value>,
    audio: Option<Vec<serde_json::Value>>,
    dolby: Option<DolbyData>,
    flac: Option<FlacData>,
}

#[derive(Debug, Deserialize)]
struct DolbyData {
    audio: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct FlacData {
    audio: Option<serde_json::Value>,
}

fn parse_video_track(value: &serde_json::Value) -> anyhow::Result<VideoTrack> {
    let id = value_u32(value, "id")?;
    let codecs = value_string_opt(value, "codecs").unwrap_or_default();
    let codecid = value_u32_opt(value, "codecid");
    Ok(VideoTrack {
        quality_id: id,
        quality_name: quality_name(id).to_string(),
        base_url: value_string_any(value, &["base_url", "baseUrl"])?,
        backup_urls: value_string_array_any(value, &["backup_url", "backupUrl"]),
        codec_name: video_codec_name(codecid, &codecs),
        codec_id: codecid,
        bandwidth: value_u64_opt(value, "bandwidth").unwrap_or_default(),
        width: value_u32_opt(value, "width"),
        height: value_u32_opt(value, "height"),
        frame_rate: value_string_any_opt(value, &["frame_rate", "frameRate"]),
        size: value_u64_opt(value, "size"),
    })
}

fn parse_audio_track(value: &serde_json::Value) -> anyhow::Result<AudioTrack> {
    let codecs = value_string_opt(value, "codecs").unwrap_or_default();
    Ok(AudioTrack {
        id: value_u32(value, "id")?,
        base_url: value_string_any(value, &["base_url", "baseUrl"])?,
        backup_urls: value_string_array_any(value, &["backup_url", "backupUrl"]),
        codec_name: audio_codec_name(&codecs),
        bandwidth: value_u64_opt(value, "bandwidth").unwrap_or_default(),
        size: value_u64_opt(value, "size"),
    })
}

fn value_string_any(value: &serde_json::Value, keys: &[&str]) -> anyhow::Result<String> {
    value_string_any_opt(value, keys)
        .with_context(|| format!("missing string field {}", keys.join("/")))
}

fn value_string_any_opt(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(|v| v.as_str()))
        .map(ToString::to_string)
}

fn value_string_opt(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(ToString::to_string)
}

fn value_string_array_any(value: &serde_json::Value, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(|v| v.as_array()))
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn value_u32(value: &serde_json::Value, key: &str) -> anyhow::Result<u32> {
    value_u32_opt(value, key).with_context(|| format!("missing numeric field {key}"))
}

fn value_u32_opt(value: &serde_json::Value, key: &str) -> Option<u32> {
    value
        .get(key)?
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
}

fn value_u64_opt(value: &serde_json::Value, key: &str) -> Option<u64> {
    value.get(key)?.as_u64()
}
