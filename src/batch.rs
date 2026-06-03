use regex::Regex;
use serde::Deserialize;

use crate::{client::BiliClient, input::parse_video_input};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchInput {
    Single(crate::input::VideoInput),
    Collection {
        sid: u64,
    },
    Favorite {
        media_id: u64,
        owner_mid: Option<u64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchVideo {
    pub aid: u64,
    pub bvid: String,
    pub title: String,
}

pub fn parse_batch_input(input: &str) -> anyhow::Result<BatchInput> {
    let input = input.trim();

    let collection_re =
        Regex::new(r"(?i)(?:collectiondetail\?sid=|[?&]sid=|listBizId:)(\d+)").unwrap();
    if (input.contains("collection") || input.contains("listBizId:"))
        && let Some(caps) = collection_re.captures(input)
    {
        return Ok(BatchInput::Collection {
            sid: caps[1].parse()?,
        });
    }

    let fav_re = Regex::new(r"(?i)(?:favlist\?fid=|[?&]fid=|favId:)(\d+)").unwrap();
    if (input.contains("favlist") || input.contains("favId:"))
        && let Some(caps) = fav_re.captures(input)
    {
        let owner_mid = Regex::new(r"space\.bilibili\.com/(\d+)")
            .unwrap()
            .captures(input)
            .map(|caps| caps[1].parse())
            .transpose()?;
        return Ok(BatchInput::Favorite {
            media_id: caps[1].parse()?,
            owner_mid,
        });
    }

    Ok(BatchInput::Single(parse_video_input(input)?))
}

pub async fn fetch_batch_videos(
    client: &BiliClient,
    input: &BatchInput,
) -> anyhow::Result<Vec<BatchVideo>> {
    match input {
        BatchInput::Single(video) => {
            let info = crate::video::fetch_video_info(client, video).await?;
            Ok(vec![BatchVideo {
                aid: info.aid,
                bvid: info.bvid,
                title: info.title,
            }])
        }
        BatchInput::Collection { sid } => fetch_collection(client, *sid).await,
        BatchInput::Favorite { media_id, .. } => fetch_favorite(client, *media_id).await,
    }
}

async fn fetch_collection(client: &BiliClient, sid: u64) -> anyhow::Result<Vec<BatchVideo>> {
    let mut page_num = 1;
    let mut out = Vec::new();
    loop {
        let url = format!(
            "https://api.bilibili.com/x/polymer/web-space/seasons_archives_list?season_id={sid}&page_num={page_num}&page_size=30"
        );
        let resp: CollectionResponse = client.get_json(&url).await?;
        if resp.code != 0 {
            anyhow::bail!(
                "collection API failed: code={}, message={}",
                resp.code,
                resp.message.unwrap_or_default()
            );
        }
        let data = match resp.data {
            Some(data) => data,
            None => break,
        };
        let archives = data.archives.unwrap_or_default();
        if archives.is_empty() {
            break;
        }
        out.extend(archives.into_iter().filter_map(|item| {
            Some(BatchVideo {
                aid: item.aid?,
                bvid: item.bvid?,
                title: item.title?,
            })
        }));
        if page_num >= data.page.map(|p| p.page_count).unwrap_or(page_num) {
            break;
        }
        page_num += 1;
    }
    Ok(out)
}

async fn fetch_favorite(client: &BiliClient, media_id: u64) -> anyhow::Result<Vec<BatchVideo>> {
    let mut pn = 1;
    let mut out = Vec::new();
    loop {
        let url = format!(
            "https://api.bilibili.com/x/v3/fav/resource/list?media_id={media_id}&pn={pn}&ps=20"
        );
        let resp: FavoriteResponse = client.get_json(&url).await?;
        if resp.code != 0 {
            anyhow::bail!(
                "favorite API failed: code={}, message={}",
                resp.code,
                resp.message.unwrap_or_default()
            );
        }
        let data = match resp.data {
            Some(data) => data,
            None => break,
        };
        let medias = data.medias.unwrap_or_default();
        if medias.is_empty() {
            break;
        }
        out.extend(medias.into_iter().filter_map(|item| {
            Some(BatchVideo {
                aid: item.id?,
                bvid: item.bvid?,
                title: item.title?,
            })
        }));
        if !data.has_more.unwrap_or(false) {
            break;
        }
        pn += 1;
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct CollectionResponse {
    code: i64,
    message: Option<String>,
    data: Option<CollectionData>,
}

#[derive(Debug, Deserialize)]
struct CollectionData {
    archives: Option<Vec<CollectionArchive>>,
    page: Option<CollectionPage>,
}

#[derive(Debug, Deserialize)]
struct CollectionPage {
    #[serde(default)]
    page_count: u64,
}

#[derive(Debug, Deserialize)]
struct CollectionArchive {
    aid: Option<u64>,
    bvid: Option<String>,
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FavoriteResponse {
    code: i64,
    message: Option<String>,
    data: Option<FavoriteData>,
}

#[derive(Debug, Deserialize)]
struct FavoriteData {
    medias: Option<Vec<FavoriteMedia>>,
    has_more: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct FavoriteMedia {
    id: Option<u64>,
    bvid: Option<String>,
    title: Option<String>,
}
