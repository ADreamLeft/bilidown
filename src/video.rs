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
        video_tracks: dash.video.into_iter().map(Into::into).collect(),
        audio_tracks: audio.into_iter().map(Into::into).collect(),
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
    video: Vec<ApiVideoTrack>,
    audio: Option<Vec<ApiAudioTrack>>,
    dolby: Option<DolbyData>,
    flac: Option<FlacData>,
}

#[derive(Debug, Deserialize)]
struct DolbyData {
    audio: Option<Vec<ApiAudioTrack>>,
}

#[derive(Debug, Deserialize)]
struct FlacData {
    audio: Option<ApiAudioTrack>,
}

#[derive(Debug, Deserialize)]
struct ApiVideoTrack {
    id: u32,
    #[serde(alias = "baseUrl", alias = "base_url")]
    base_url: String,
    #[serde(default, alias = "backupUrl", alias = "backup_url")]
    backup_urls: Vec<String>,
    #[serde(default)]
    bandwidth: u64,
    #[serde(default)]
    codecid: Option<u32>,
    #[serde(default)]
    codecs: String,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default, alias = "frameRate", alias = "frame_rate")]
    frame_rate: Option<String>,
    #[serde(default)]
    size: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ApiAudioTrack {
    id: u32,
    #[serde(alias = "baseUrl", alias = "base_url")]
    base_url: String,
    #[serde(default, alias = "backupUrl", alias = "backup_url")]
    backup_urls: Vec<String>,
    #[serde(default)]
    bandwidth: u64,
    #[serde(default)]
    codecs: String,
    #[serde(default)]
    size: Option<u64>,
}

impl From<ApiVideoTrack> for VideoTrack {
    fn from(value: ApiVideoTrack) -> Self {
        Self {
            quality_id: value.id,
            quality_name: quality_name(value.id).to_string(),
            base_url: value.base_url,
            backup_urls: value.backup_urls,
            codec_name: video_codec_name(value.codecid, &value.codecs),
            codec_id: value.codecid,
            bandwidth: value.bandwidth,
            width: value.width,
            height: value.height,
            frame_rate: value.frame_rate,
            size: value.size,
        }
    }
}

impl From<ApiAudioTrack> for AudioTrack {
    fn from(value: ApiAudioTrack) -> Self {
        Self {
            id: value.id,
            base_url: value.base_url,
            backup_urls: value.backup_urls,
            codec_name: audio_codec_name(&value.codecs),
            bandwidth: value.bandwidth,
            size: value.size,
        }
    }
}
