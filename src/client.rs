use std::{
    fs::File,
    io::{BufReader, BufWriter},
    path::PathBuf,
    sync::Arc,
};

use anyhow::Context;
use reqwest::header::{HeaderMap, HeaderValue, REFERER, USER_AGENT};
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
use serde::Deserialize;

use crate::{REFERER as BILI_REFERER, USER_AGENT as BILI_USER_AGENT, config, wbi};

#[derive(Clone)]
pub struct BiliClient {
    http: reqwest::Client,
    cookie_store: Arc<CookieStoreMutex>,
    cookie_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct LoginStatus {
    pub is_login: bool,
    pub uname: Option<String>,
    pub mid: Option<u64>,
}

impl BiliClient {
    pub fn new() -> anyhow::Result<Self> {
        let cookie_path = config::cookie_path()?;
        let cookie_store = load_cookie_store(&cookie_path)?;
        let cookie_store = Arc::new(CookieStoreMutex::new(cookie_store));

        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(BILI_USER_AGENT));
        headers.insert(REFERER, HeaderValue::from_static(BILI_REFERER));

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .cookie_provider(Arc::clone(&cookie_store))
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .build()
            .context("build reqwest client")?;

        Ok(Self {
            http,
            cookie_store,
            cookie_path,
        })
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub fn save_cookies(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.cookie_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::create(&self.cookie_path)
            .with_context(|| format!("create cookie file {}", self.cookie_path.display()))?;
        let mut writer = BufWriter::new(file);
        let store = self.cookie_store.lock().unwrap();
        cookie_store::serde::json::save_incl_expired_and_nonpersistent(&store, &mut writer)
            .map_err(|err| anyhow::anyhow!("save cookie store: {err}"))?;
        Ok(())
    }

    /// 确保 cookie 中存在 buvid3：搜索等接口缺少它会返回 -412 被拦截。
    /// 缺失时访问一次 Web 首页，让服务端通过 Set-Cookie 写入 buvid3 并落盘。
    pub async fn ensure_buvid(&self) -> anyhow::Result<()> {
        let has_buvid = {
            let store = self.cookie_store.lock().unwrap();
            store.iter_any().any(|cookie| cookie.name() == "buvid3")
        };
        if has_buvid {
            return Ok(());
        }
        let _ = self.get_text("https://www.bilibili.com/").await;
        self.save_cookies()?;
        Ok(())
    }

    pub async fn get_text(&self, url: &str) -> anyhow::Result<String> {
        let resp = self
            .http
            .get(url)
            .header(REFERER, BILI_REFERER)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?
            .error_for_status()
            .with_context(|| format!("GET {url} returned error status"))?;
        Ok(resp.text().await?)
    }

    pub async fn get_json<T: for<'de> Deserialize<'de>>(&self, url: &str) -> anyhow::Result<T> {
        let text = self.get_text(url).await?;
        serde_json::from_str(&text).with_context(|| format!("parse JSON response from {url}"))
    }

    pub async fn status(&self) -> anyhow::Result<LoginStatus> {
        let nav: NavResponse = self
            .get_json("https://api.bilibili.com/x/web-interface/nav")
            .await?;
        if nav.code != 0 {
            anyhow::bail!(
                "nav API failed: code={}, message={}",
                nav.code,
                nav.message.unwrap_or_default()
            );
        }
        let data = nav.data.context("nav API returned no data")?;
        Ok(LoginStatus {
            is_login: data.is_login,
            uname: data.uname,
            mid: data.mid,
        })
    }

    pub async fn fetch_wbi_key(&self) -> anyhow::Result<String> {
        let nav: NavResponse = self
            .get_json("https://api.bilibili.com/x/web-interface/nav")
            .await?;
        let data = nav.data.context("nav API returned no data")?;
        let wbi_img = data.wbi_img.context("nav API returned no WBI image keys")?;
        let img_key = key_from_url(&wbi_img.img_url)?;
        let sub_key = key_from_url(&wbi_img.sub_url)?;
        Ok(wbi::mixin_key(&img_key, &sub_key))
    }
}

fn load_cookie_store(path: &PathBuf) -> anyhow::Result<CookieStore> {
    if !path.exists() {
        return Ok(CookieStore::default());
    }
    let file = File::open(path).with_context(|| format!("open cookie file {}", path.display()))?;
    let reader = BufReader::new(file);
    cookie_store::serde::json::load_all(reader)
        .map_err(|err| anyhow::anyhow!("load cookie store: {err}"))
}

fn key_from_url(raw: &str) -> anyhow::Result<String> {
    let last = raw
        .rsplit('/')
        .next()
        .context("invalid WBI key URL")?
        .split('?')
        .next()
        .unwrap_or_default();
    let key = last
        .rsplit_once('.')
        .map(|(key, _)| key)
        .unwrap_or(last)
        .to_string();
    if key.is_empty() {
        anyhow::bail!("empty WBI key in URL: {raw}");
    }
    Ok(key)
}

#[derive(Debug, Deserialize)]
struct NavResponse {
    code: i64,
    message: Option<String>,
    data: Option<NavData>,
}

#[derive(Debug, Deserialize)]
struct NavData {
    #[serde(rename = "isLogin")]
    is_login: bool,
    uname: Option<String>,
    mid: Option<u64>,
    #[serde(default)]
    wbi_img: Option<WbiImg>,
}

#[derive(Debug, Deserialize)]
struct WbiImg {
    img_url: String,
    sub_url: String,
}
