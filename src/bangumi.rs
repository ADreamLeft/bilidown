use anyhow::Context;
use regex::Regex;
use serde::Deserialize;

use crate::client::BiliClient;

/// 番剧/影视输入：剧集 ep 或整季 ss
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BangumiInput {
    Ep(u64),
    Season(u64),
}

/// 从 `ep123` / `ss123` / 番剧播放页 URL 中解析出 ep_id 或 season_id。
/// 普通投稿（BV/av/视频 URL）不会被误判为番剧。
pub fn parse_bangumi_input(raw: &str) -> Option<BangumiInput> {
    let s = raw.trim();
    let ep_re = Regex::new(r"(?i)(?:^ep|bangumi/play/ep|ep_id=)(\d+)").unwrap();
    let ss_re = Regex::new(r"(?i)(?:^ss|bangumi/play/ss|season_id=)(\d+)").unwrap();
    if let Some(c) = ep_re.captures(s)
        && let Ok(id) = c[1].parse()
    {
        return Some(BangumiInput::Ep(id));
    }
    if let Some(c) = ss_re.captures(s)
        && let Ok(id) = c[1].parse()
    {
        return Some(BangumiInput::Season(id));
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Season {
    pub title: String,
    pub episodes: Vec<Episode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Episode {
    pub index: usize,
    pub ep_id: u64,
    pub aid: u64,
    pub cid: u64,
    pub bvid: String,
    pub title: String,
    pub long_title: String,
    pub cover: String,
}

pub async fn fetch_season(client: &BiliClient, input: &BangumiInput) -> anyhow::Result<Season> {
    let url = match input {
        BangumiInput::Ep(id) => {
            format!("https://api.bilibili.com/pgc/view/web/season?ep_id={id}")
        }
        BangumiInput::Season(id) => {
            format!("https://api.bilibili.com/pgc/view/web/season?season_id={id}")
        }
    };
    let text = client.get_text(&url).await?;
    parse_season_response(&text)
}

pub fn parse_season_response(text: &str) -> anyhow::Result<Season> {
    let resp: SeasonResponse = serde_json::from_str(text).context("parse season response")?;
    if resp.code != 0 {
        anyhow::bail!(
            "season API failed: code={}, message={}",
            resp.code,
            resp.message.unwrap_or_default()
        );
    }
    let result = resp.result.context("season API returned no data")?;
    let title = result
        .season_title
        .filter(|s| !s.is_empty())
        .or(result.title)
        .unwrap_or_default();
    let episodes = result
        .episodes
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(idx, ep)| Episode {
            index: idx + 1,
            ep_id: ep.id,
            aid: ep.aid.unwrap_or_default(),
            cid: ep.cid.unwrap_or_default(),
            bvid: ep.bvid.unwrap_or_default(),
            title: ep.title.unwrap_or_default(),
            long_title: ep.long_title.unwrap_or_default(),
            cover: ep.cover.unwrap_or_default(),
        })
        .collect();
    Ok(Season { title, episodes })
}

#[derive(Debug, Deserialize)]
struct SeasonResponse {
    code: i64,
    message: Option<String>,
    result: Option<SeasonResult>,
}

#[derive(Debug, Deserialize)]
struct SeasonResult {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    season_title: Option<String>,
    #[serde(default)]
    episodes: Option<Vec<ApiEpisode>>,
}

#[derive(Debug, Deserialize)]
struct ApiEpisode {
    /// 剧集 ep_id
    id: u64,
    #[serde(default)]
    aid: Option<u64>,
    #[serde(default)]
    cid: Option<u64>,
    #[serde(default)]
    bvid: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    long_title: Option<String>,
    #[serde(default)]
    cover: Option<String>,
}
