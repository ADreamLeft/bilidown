# 项目结构与系统架构

## 模块划分

项目按职责拆成 17 个模块（`src/*.rs`），每个模块只负责一件事：

| 模块 | 职责 |
|---|---|
| `main.rs` / `lib.rs` | 入口与模块声明 |
| `cli.rs` | 用 `clap` 定义命令行参数与子命令，分发到 `commands` |
| `commands.rs` | 各子命令的业务编排（下载流程的总调度在这里） |
| `client.rs` | 封装 `reqwest` 客户端、cookie 持久化、WBI key 获取 |
| `auth.rs` | 扫码登录与登录态查询 |
| `wbi.rs` | WBI 签名算法 |
| `input.rs` / `batch.rs` | 解析普通视频输入 / 合集、收藏夹批量输入 |
| `video.rs` | 普通视频与番剧的元数据、playurl 解析 |
| `bangumi.rs` | 番剧（PGC）输入解析与剧集列表获取 |
| `search.rs` | 视频 / 用户搜索 |
| `page.rs` | 分 P / 剧集选择表达式（`1,3-5,all`） |
| `download.rs` | 并发分片下载、断点续传、重试、备用 URL |
| `assets.rs` | 封面、字幕、弹幕下载 |
| `mux.rs` | 调用 `ffmpeg` 合并音视频 |
| `archive.rs` | 下载归档（去重） |
| `config.rs` | TOML 配置读写 |
| `fs_utils.rs` | 输出路径模板渲染、文件名清洗 |

这种"一个模块一个职责"的划分让依赖关系保持单向、清晰：`cli → commands → {client, video, bangumi, search, download, assets, mux, archive}`，底层模块（`wbi`、`fs_utils`、`page`）不反向依赖上层。

## 一次下载的数据流

以 `bilidown download BV1xxxxxxx --page all` 为例：

```text
命令行参数
  └─ cli.rs        解析参数，构造 DownloadOptions
      └─ commands.rs  download()：先判断输入是普通视频还是番剧
          ├─ input.rs / batch.rs   解析 BV/av/URL/合集/收藏夹
          ├─ client.rs             准备带 cookie 的 reqwest 客户端
          ├─ video.rs              取视频信息 + WBI 签名后请求 playurl，解析 DASH 流
          ├─ page.rs               按 --page 选出要下载的分 P
          └─ 对每个分 P：download_source()
               ├─ video.rs         选出最优视频/音频流（按画质、编码、码率）
               ├─ download.rs      并发 Range 分片下载音视频流（带续传/重试/备用URL）
               ├─ assets.rs        可选：封面 / 字幕 / 弹幕
               ├─ mux.rs           ffmpeg -c copy 合并为 mp4
               └─ archive.rs       记录已完成的 aid/cid（用于去重）
```

## UGC 与 PGC 的统一

普通投稿（UGC）和番剧（PGC）走的是**两套不同的 B 站接口**：

- UGC：`/x/web-interface/view` 取信息、`/x/player/wbi/playurl`（需 WBI 签名）取流；
- PGC：`/pgc/view/web/season` 取剧集、`/pgc/player/web/playurl`（无需 WBI）取流。

但拿到音视频流之后，**下载、合并、附加资源、归档的逻辑完全一致**。为了避免重复代码，我把这部分抽象成一个共用函数 `download_source()`，它接收一个 `StreamSource`（已解析好的流 + 模板变量），UGC 的 `download_page()` 和 PGC 的 `download_episode()` 各自构造 `StreamSource` 后调用它：

```rust
struct StreamSource {
    parsed: ParsedPlay,   // 已解析的音视频流
    label: String,        // 进度展示前缀，如 "P1" / "EP1"
    title: String, part: String, bvid: String,
    aid: u64, cid: u64, owner: String, cover_url: String,
}

async fn download_source(client: &BiliClient, source: &StreamSource, opts: &DownloadOptions)
    -> anyhow::Result<Option<ArchiveEntry>> { /* 选流 → 下载 → 资源 → 合并 → 归档 */ }
```

这样新增番剧支持时，下载主流程一行都没改，只是多了一个"构造 `StreamSource`"的入口——这也是模块化设计带来的直接好处。下一章详细讲这些设计里用到的 Rust 语言特性。
