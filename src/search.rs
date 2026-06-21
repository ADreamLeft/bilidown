use std::collections::BTreeMap;

use anyhow::Context;
use serde::Deserialize;

use crate::{client::BiliClient, wbi};

/// 搜索目标类型，对应接口的 `search_type`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchType {
    Video,
    User,
}

impl SearchType {
    fn api_value(self) -> &'static str {
        match self {
            SearchType::Video => "video",
            SearchType::User => "bili_user",
        }
    }
}

/// 把用户给的排序名解析成接口的 `order` 取值（取值随搜索类型不同而不同）
pub fn resolve_order(search_type: SearchType, order: &str) -> anyhow::Result<String> {
    let key = order.trim().to_ascii_lowercase();
    let value = match search_type {
        SearchType::Video => match key.as_str() {
            "" | "default" | "comprehensive" | "totalrank" => "totalrank",
            "play" | "click" => "click",
            "new" | "pubdate" => "pubdate",
            "danmaku" | "dm" => "dm",
            "favorite" | "fav" | "stow" => "stow",
            "comment" | "scores" => "scores",
            other => anyhow::bail!(
                "video 搜索不支持的排序：{other}（可用 default/play/new/danmaku/favorite/comment）"
            ),
        },
        SearchType::User => match key.as_str() {
            "" | "default" | "0" => "0",
            "fans" => "fans",
            "level" => "level",
            other => {
                anyhow::bail!("user 搜索不支持的排序：{other}（可用 default/fans/level）")
            }
        },
    };
    Ok(value.to_string())
}

/// 视频时长过滤，对应接口的 `duration`（0 全部，1 <10min，2 10-30min，3 30-60min，4 >60min）
pub fn resolve_duration(duration: &str) -> anyhow::Result<u32> {
    Ok(match duration.trim().to_ascii_lowercase().as_str() {
        "" | "all" | "0" => 0,
        "short" | "1" => 1,
        "medium" | "2" => 2,
        "long" | "3" => 3,
        "verylong" | "4" => 4,
        other => {
            anyhow::bail!("不支持的时长过滤：{other}（可用 all/short/medium/long/verylong）")
        }
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchResult {
    Video(VideoResult),
    User(UserResult),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoResult {
    pub bvid: String,
    pub aid: u64,
    pub title: String,
    pub author: String,
    pub mid: u64,
    pub duration: String,
    pub play: i64,
    pub danmaku: i64,
    pub pubdate: i64,
    pub typename: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserResult {
    pub mid: u64,
    pub uname: String,
    pub fans: i64,
    pub videos: i64,
    pub level: i64,
    pub sign: String,
}

pub struct SearchParams<'a> {
    pub keyword: &'a str,
    pub search_type: SearchType,
    pub order: String,
    pub duration: u32,
    pub page: u32,
    pub page_size: u32,
}

pub async fn search(
    client: &BiliClient,
    params: &SearchParams<'_>,
) -> anyhow::Result<Vec<SearchResult>> {
    // 搜索接口强制要求 buvid3 cookie，否则返回 -412 被拦截
    client.ensure_buvid().await?;
    let wbi_key = client.fetch_wbi_key().await?;

    let mut query = BTreeMap::new();
    query.insert(
        "search_type".to_string(),
        params.search_type.api_value().to_string(),
    );
    query.insert("keyword".to_string(), params.keyword.to_string());
    query.insert("page".to_string(), params.page.to_string());
    query.insert("page_size".to_string(), params.page_size.to_string());
    if !params.order.is_empty() {
        query.insert("order".to_string(), params.order.clone());
    }
    if params.search_type == SearchType::Video && params.duration > 0 {
        query.insert("duration".to_string(), params.duration.to_string());
    }

    let signed = wbi::sign_params(query, &wbi_key).to_query_string();
    let url = format!("https://api.bilibili.com/x/web-interface/wbi/search/type?{signed}");
    let text = client.get_text(&url).await?;
    parse_search_response(&text, params.search_type)
}

pub fn parse_search_response(
    text: &str,
    search_type: SearchType,
) -> anyhow::Result<Vec<SearchResult>> {
    let resp: SearchResponse = serde_json::from_str(text).context("parse search response")?;
    if resp.code != 0 {
        anyhow::bail!(
            "search API failed: code={}, message={}",
            resp.code,
            resp.message.unwrap_or_default()
        );
    }
    let data = resp.data.context("search API returned no data")?;
    let items = data.result.unwrap_or_default();

    let results = items
        .into_iter()
        .filter_map(|item| match search_type {
            SearchType::Video => {
                let bvid = item.bvid.unwrap_or_default();
                if bvid.is_empty() {
                    return None;
                }
                Some(SearchResult::Video(VideoResult {
                    bvid,
                    aid: item.aid.unwrap_or_default(),
                    title: strip_html(&item.title.unwrap_or_default()),
                    author: item.author.unwrap_or_default(),
                    mid: item.mid.unwrap_or_default(),
                    duration: item.duration.unwrap_or_default(),
                    play: item.play.unwrap_or_default(),
                    danmaku: item.video_review.or(item.danmaku).unwrap_or_default(),
                    pubdate: item.pubdate.or(item.senddate).unwrap_or_default(),
                    typename: item.typename.unwrap_or_default(),
                }))
            }
            SearchType::User => {
                let mid = item.mid.unwrap_or_default();
                if mid == 0 {
                    return None;
                }
                Some(SearchResult::User(UserResult {
                    mid,
                    uname: strip_html(&item.uname.unwrap_or_default()),
                    fans: item.fans.unwrap_or_default(),
                    videos: item.videos.unwrap_or_default(),
                    level: item.level.unwrap_or_default(),
                    sign: item.usign.unwrap_or_default(),
                }))
            }
        })
        .collect();
    Ok(results)
}

/// 去掉搜索结果里 `<em class="keyword">…</em>` 这类高亮标签，并解码常见 HTML 实体
fn strip_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    code: i64,
    message: Option<String>,
    data: Option<SearchData>,
}

#[derive(Debug, Deserialize)]
struct SearchData {
    #[serde(default)]
    result: Option<Vec<SearchItem>>,
}

#[derive(Debug, Deserialize)]
struct SearchItem {
    // 视频字段
    bvid: Option<String>,
    aid: Option<u64>,
    title: Option<String>,
    author: Option<String>,
    duration: Option<String>,
    play: Option<i64>,
    video_review: Option<i64>,
    danmaku: Option<i64>,
    pubdate: Option<i64>,
    senddate: Option<i64>,
    typename: Option<String>,
    // 用户字段
    uname: Option<String>,
    usign: Option<String>,
    fans: Option<i64>,
    videos: Option<i64>,
    level: Option<i64>,
    // 公共字段
    mid: Option<u64>,
}
