# 关键 Rust 设计

这一章挑出项目里最能体现 Rust 语言特性的几处设计。

## 错误处理：`anyhow::Result` + `?`

下载流程的每一步都可能失败：网络请求超时、HTTP 状态异常、JSON 字段缺失、`ffmpeg` 退出码非零、文件读写错误……项目用 [`anyhow`](https://docs.rs/anyhow) 统一错误类型，几乎所有可能失败的函数都返回 `anyhow::Result<T>`，用 `?` 向上传播，并用 `.context()` / `.with_context()` 附加可读的上下文：

```rust
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
```

对 API 返回码这种"逻辑错误"，用 `anyhow::bail!` / `ensure!` 提前返回：

```rust
if resp.code != 0 {
    anyhow::bail!("video info API failed: code={}, message={}", resp.code,
                  resp.message.unwrap_or_default());
}
```

整个项目 **0 处 `unsafe`**，`unwrap()` 只出现在编译期就能保证不 panic 的地方（如静态正则、进度条模板）。错误信息逐层累积，最终在命令行打印出一条带完整上下文的链路，便于排查。

## 异步与并发：`tokio` + Range 分片下载

下载大文件时，bilidown 先用一个 `bytes=0-0` 的探测请求确认服务器支持 Range 并拿到总大小，然后把文件按连接数切成若干区间，用 `tokio` 并发下载每一段，最后按序拼接：

```rust
let ranges = split_ranges(total, config.connections);   // 把 [0,total) 均分为 N 段
let mut tasks = Vec::new();
for (idx, (start, end)) in ranges.into_iter().enumerate() {
    let path = parts_dir.join(format!("{idx:05}.part"));
    let (url, client, pb) = (url.to_string(), client.clone(), pb.clone());
    tasks.push(async move {
        download_part(&client, &url, &path, start, end, config.retries, pb).await
    });
}
futures_util::future::try_join_all(tasks).await?;        // 任一分片失败即整体失败
```

`try_join_all` 并发驱动所有分片任务，任意一个返回 `Err` 就立刻短路。每个分片是一个独立的 `Range` 请求，写入各自的 `.part` 文件，互不干扰。这背后是 Rust 的 `Future` + `async move` 闭包捕获各自数据的所有权，编译器静态保证了任务之间没有数据竞争。

## 断点续传与可靠性

下载是最容易中途失败的环节，项目做了三层保障：

- **断点续传**：顺序下载时读取已有临时文件大小，发 `Range: bytes={existing}-` 续传并追加；分片下载时每个 `.part` 也各自检查已下大小、只补缺失部分。
- **失败重试**：`send_with_retries` 带退避重试（`250ms * 第几次`）。
- **备用 URL**：B 站会返回多个 CDN 地址，主地址失败时自动 fallback 到备用地址。

写入采用"先写临时文件、成功后 `rename` 到目标名"的原子落盘，配合"目标文件已存在则跳过"，整个下载是**幂等**的——同一条命令重复执行不会重复下载或产生半成品。

## 所有权与资源管理

下载器要同时管理网络响应流、临时分片文件、最终输出文件、配置文件和 cookie store。Rust 的所有权模型让这些资源在离开作用域时自动释放，无需手写清理。

一个典型例子是 cookie store 的共享：它既要被我们读写（保存登录态），又要交给 `reqwest` 客户端自动带 cookie。用 `Arc<CookieStoreMutex>` 共享所有权，再 `Arc::clone` 一份给客户端：

```rust
let cookie_store = Arc::new(CookieStoreMutex::new(load_cookie_store(&cookie_path)?));
let http = reqwest::Client::builder()
    .cookie_provider(Arc::clone(&cookie_store))   // 客户端持有一份
    .build()?;
Ok(Self { http, cookie_store, cookie_path })       // 我们自己也持有一份
```

锁的获取被限制在最小作用域内，避免跨 `.await` 持锁导致的死锁——例如检查 buvid3 cookie 时，先在一个 `{}` 块里取锁判断、立刻释放，再去发网络请求。

## 枚举与模式匹配

项目用 `enum` 刻画各种"有限状态"，再用 `match` 穷尽分发，让非法组合在编译期就不可表达。例如画质偏好、下载模式、搜索类型、输入类型：

```rust
pub enum QualityPreference { Best, Id(u32) }
pub enum DownloadMode { Both, Audio, Video }
pub enum SearchType { Video, User }
pub enum BangumiInput { Ep(u64), Season(u64) }
```

下载模式决定要选哪些流，一个 `match` 就把三种情况说清楚：

```rust
match mode {
    DownloadMode::Both  => SelectedDownloadTracks { video: Some(select_video()?), audio: Some(select_audio()?) },
    DownloadMode::Audio => SelectedDownloadTracks { video: None,                  audio: Some(select_audio()?) },
    DownloadMode::Video => SelectedDownloadTracks { video: Some(select_video()?), audio: None },
}
```

搜索结果用带数据的枚举区分视频和用户，打印时按变体分别格式化：

```rust
match result {
    SearchResult::Video(v) => println!("{}  UP:{}  ▶{}", v.title, v.author, v.play),
    SearchResult::User(u)  => println!("{}  粉丝:{}", u.uname, u.fans),
}
```

## Trait 与泛型

**泛型 + trait 约束** 让一个函数适配所有可反序列化的类型。客户端只写一个 `get_json`，调用处用类型标注决定解析成什么结构：

```rust
pub async fn get_json<T: for<'de> Deserialize<'de>>(&self, url: &str) -> anyhow::Result<T> {
    let text = self.get_text(url).await?;
    serde_json::from_str(&text).with_context(|| format!("parse JSON response from {url}"))
}
```

**`From` trait** 把"贴近接口的 DTO"转换成"贴近业务的领域类型"，转换逻辑集中、调用处用 `.into()` 即可：

```rust
impl From<ApiVideoInfo> for VideoInfo {
    fn from(value: ApiVideoInfo) -> Self {
        Self { aid: value.aid, bvid: value.bvid, title: value.title,
               owner_name: value.owner.name, pages: value.pages.into_iter().map(Into::into).collect(), .. }
    }
}
```

此外，大量用到**派生宏（derive macro）**：`#[derive(Deserialize)]` 自动生成 JSON 解析、`#[derive(Parser, Subcommand, ValueEnum)]` 自动生成命令行解析。这些都是 trait 由编译器自动实现的体现。

## WBI 签名：把算法翻译成 Rust

B 站 Web 播放接口要求对查询参数做 WBI 签名：把两段密钥按一张固定的 64 元置换表重排取前 32 位得到 `mixin_key`，再把按 key 排序的参数拼成 query、追加 `mixin_key` 求 MD5 得到 `w_rid`。这段算法用迭代器写得很简洁：

```rust
pub fn mixin_key(img_key: &str, sub_key: &str) -> String {
    let orig = format!("{img_key}{sub_key}");
    MIXIN_KEY_ENC_TAB.iter()
        .filter_map(|&i| orig.as_bytes().get(i).copied())
        .take(32)
        .map(char::from)
        .collect()
}
```

参数排序直接用 `BTreeMap<String, String>`（按 key 有序），省去手动排序：

```rust
let query = encode_pairs(&pairs);            // 已排序、已 url-encode
let mut hasher = Md5::new();
hasher.update(query.as_bytes());
hasher.update(mixin_key.as_bytes());
let w_rid = format!("{:x}", hasher.finalize());
```

下一章介绍如何保证这些设计真的正确——测试、CI 与真实接口验证。
