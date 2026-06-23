# 功能说明与使用方法

为了让读者快速了解 bilidown 的功能和使用方法，这一章将从用户视角介绍 bilidown 能做什么、怎么用。（以下图标是 AI 加的，我感觉还合适，就保留了，老师别挂我😭）

## 功能概览

- 🔑 扫码登录，复用本机登录态
- 🔍 搜索视频与 UP 主（多种排序、时长过滤）
- 📺 下载普通视频、合集、收藏夹
- 🎞️ 下载番剧 / 影视（`ep` / `ss`）
- 🎯 分 P / 剧集选择：`1`、`1,3-5`、`all`
- 🧩 画质、编码、音频质量偏好
- 🚀 并发 Range 分片下载、断点续传、失败重试、备用 URL fallback
- 📎 下载封面、字幕（转 SRT）、弹幕
- 🎵 `--audio-only` / `--video-only`
- 🗂️ 默认配置 + 下载归档去重

## 命令一览

bilidown 支持以下子命令：

| 命令 | 说明 |
|---|---|
| `login` | 扫码登录并保存登录态 |
| `status` | 查看当前登录态 |
| `search <关键词>` | 搜索视频或 UP 主 |
| `info <输入>` | 查看视频分 P / 番剧剧集与可用音视频流 |
| `download <输入>` | 下载视频、合集、收藏夹或番剧 |
| `config <子命令>` | 查看 / 修改默认配置 |
| `archive <子命令>` | 查看 / 清空下载归档 |

其中 `<输入>` 可以是 BV 号、av 号、视频 URL、合集 / 收藏夹链接，或番剧 `ep<id>` / `ss<id>` / 播放页 URL——程序会自动识别类型并分流。

## 典型用法

### 登录与搜索

```bash
bilidown login                                   # 终端显示二维码，用 App 扫码
bilidown search 编程 --order play -n 5           # 按播放量搜视频，取前 5 条
bilidown search 老番茄 --type user --order fans  # 搜 UP 主，按粉丝数排序
```

搜索结果会给出 BVID，可直接喂给 `download`。视频排序支持 `default / play / new / danmaku / favorite / comment`，用户排序支持 `default / fans / level`。

其中，`--order` 是排序方式，`-n` 是结果条数，`--type` 是搜索类型（视频 / 用户）。二维码这个登录方式参考自 BBdown，我觉得比 Cookie 方便多了，而且可以复用本机登录态。

### 下载普通视频

```bash
bilidown download BV1xxxxxxx                                  # 第 1 个分 P
bilidown download BV1xxxxxxx --page all                       # 全部分 P
bilidown download BV1xxxxxxx --quality 1080 --codec av1,hevc,avc -o ./videos
bilidown download BV1xxxxxxx --audio-only                     # 只下音频
bilidown download BV1xxxxxxx --all-assets                     # 封面 + 字幕 + 弹幕
```

下载时可以精细控制**清晰度、视频编码、音频质量**三类偏好。

- **清晰度 `--quality`**：平时用默认的 `best` 即可（自动选当前可用的最高画质）；想指定时**直接写分辨率**就行，常见几档：

  | 画质 | `--quality` |
  |---|---|
  | 360P | `360` |
  | 480P | `480` |
  | 720P | `720` |
  | 1080P | `1080` |

  更高阶的用名字指定：4K `4k`、HDR `hdr`、杜比视界 `dolby`、8K `8k`（另有 1080P 高码率 `1080+`、1080P60 `1080p60`）。这些通常需要登录、部分需大会员；也兼容原始 qn 数字（如 `80`）。无权限时 `best` 会自动回退到可用的最高画质。

- **视频编码 `--codec`**：按优先级列表挑选，默认 `av1,hevc,avc`。同一清晰度往往同时有多种编码，程序按这个顺序选第一个可用的

- **音频质量 `--audio-quality`**：`best`（默认，选码率最高的音轨），或用质量名 `high` / `medium` / `low`；番剧或高码率视频还可能提供**杜比全景声**（`dolby`）和 **Hi-Res 无损 FLAC**（`flac`），也能直接指定。同样兼容原始音频 id（如 `30280`）。

**选择逻辑**：程序先把视频流按清晰度从高到低排序，再按 `--codec` 偏好、最后按码率，挑出“最优视频流”；音频则直接按码率最高优先。拿不准某个视频到底有哪些流时，先用 `bilidown info BV1xxxxxxx` 查看它实际提供的清晰度 / 编码 / 音频列表，再决定参数。


### 下载番剧

```bash
bilidown info ss33802                 # 列出整季剧集
bilidown download ep374660           # 下指定一集
bilidown download ss33802 --page all # 下整季
bilidown download ss33802 --page 1,3-5
```

我其实不看番，但是好像大家都支持了，就让 Claude Code 帮我加上了番剧支持。

### 批量与归档

```bash
# 合集 / 收藏夹链接可直接下，控制规模与请求间隔
bilidown download "https://space.bilibili.com/123/favlist?fid=456" --limit 10 --delay-per-page 2

# 写入归档并跳过已下载，适合定时增量备份
bilidown download ss33802 --page all --save-archive --skip-archived
```

### 输出模板

默认输出路径模板为 `{title}/P{page}-{part}-{quality}-{codec}.mp4`，可用变量包括 `{title} {page} {part} {quality} {codec} {bvid} {aid} {cid} {owner}`：

```bash
bilidown download BV1xxxxxxx --template "{owner}/{title}/P{page}-{part}-{cid}.mp4"
```

下一章详细说说这些命令背后的模块是怎么组织的。
