# bilidown

`bilidown` 是一个用 Rust 编写的 Bilibili 视频下载命令行工具，基于 Web 端 DASH 接口获取音视频流，并用 `ffmpeg` 合并输出。面向个人学习、资料备份和命令行自动化场景。

## Features

- 扫码登录，复用本机登录态
- 搜索视频与 UP 主（多种排序、时长过滤）
- 下载普通视频、合集、收藏夹，以及番剧 / 影视（EP / SS）
- 分 P / 剧集选择：`1`、`1,3-5`、`all`
- 画质、编码和音频质量偏好
- 并发 Range 分片下载、断点续传、失败重试、备用 URL fallback
- 下载封面、字幕（转 SRT）和弹幕
- `--audio-only` 和 `--video-only`
- 默认配置与下载归档去重

## Requirements

- Rust stable（edition 2024）
- `ffmpeg`：需在 `PATH` 中，或下载时用 `--ffmpeg-path` 指定

## Installation

```bash
cargo install --path .
# 或开发时直接运行
cargo run -- --help
```

## Commands

| 命令 | 说明 |
|------|------|
| `login` | 扫码登录并保存登录态 |
| `status` | 查看当前登录态 |
| `search <关键词>` | 搜索视频或 UP 主 |
| `info <输入>` | 查看视频分 P / 番剧剧集与可用音视频流 |
| `download <输入>` | 下载视频、合集、收藏夹或番剧 |
| `config <子命令>` | 查看 / 修改默认配置 |
| `archive <子命令>` | 查看 / 清空下载归档 |

`<输入>` 可以是 BV 号、av 号、视频 URL、合集 / 收藏夹链接，或番剧 `ep<id>` / `ss<id>` / 播放页 URL。

## Search

```bash
bilidown search 关键词                       # 搜视频，综合排序
bilidown search 关键词 --order play -n 5      # 按播放量，取前 5 条
bilidown search 关键词 --order new --duration long   # 最新发布 + 时长 30–60 分钟
bilidown search 用户名 --type user --order fans      # 搜 UP 主，按粉丝数
```

| 选项 | 取值 | 说明 |
|------|------|------|
| `-t, --type` | `video`（默认）/ `user` | 搜索类型 |
| `--order`（video） | `default` / `play` / `new` / `danmaku` / `favorite` / `comment` | 排序方式 |
| `--order`（user） | `default` / `fans` / `level` | 排序方式 |
| `--duration` | `all` / `short` / `medium` / `long` / `verylong` | 时长过滤（仅 video） |
| `-p, --page` / `--page-size` | 数字 | 翻页 |
| `-n, --limit` | 数字 | 只显示前 N 条 |

视频结果会给出 BVID，可直接用于 `download`。

## Download

```bash
# 普通视频：第 1 个分 P / 全部分 P
bilidown download BV1xxxxxxx
bilidown download BV1xxxxxxx --page all

# 指定画质、编码与输出目录
bilidown download BV1xxxxxxx --quality best --codec av1,hevc,avc -o ./videos

# 番剧：下指定一集 / 整季 / 选集
bilidown download ep374660
bilidown download ss33802 --page all
bilidown download ss33802 --page 1,3-5

# 只下音频 / 只下视频 / 附加资源
bilidown download BV1xxxxxxx --audio-only
bilidown download BV1xxxxxxx --video-only
bilidown download BV1xxxxxxx --all-assets

# 写入归档并跳过已下载 / 保留 m4s 不合并
bilidown download ss33802 --page all --save-archive --skip-archived
bilidown download BV1xxxxxxx --skip-mux
```

常用选项（完整见 `bilidown download --help`）：

| 选项 | 默认 | 说明 |
|------|------|------|
| `-p, --page` | `1` | 分 P / 剧集选择：`1`、`1,3-5`、`all` |
| `--quality` | `best` | 画质：`best` 或 qn 数字（如 `80`、`112`） |
| `--codec` | `av1,hevc,avc` | 编码优先级 |
| `--audio-quality` | `best` | 音频流：`best` 或音频 id |
| `-o, --out-dir` | `.` | 输出目录 |
| `--template` | 见下 | 输出文件名模板 |
| `--audio-only` / `--video-only` | — | 只下音频 / 只下视频 |
| `--cover` / `--subtitle` / `--danmaku` / `--all-assets` | — | 下载封面 / 字幕 / 弹幕 |
| `--connections` / `--no-multi-thread` | `8` | 并发分片连接数 / 关闭多线程 |
| `--retries` | `3` | 失败重试次数 |
| `--skip-mux` | — | 保留 m4s，不执行 ffmpeg 合并 |
| `--ffmpeg-path` | — | 指定 ffmpeg 可执行文件 |
| `--save-archive` / `--skip-archived` | — | 写入归档 / 跳过已归档 |
| `--limit` / `--delay-per-page` | — | 批量数量上限 / 每个任务间隔秒数 |

> 番剧：传 `ep<id>` 默认只下该集；传 `ss<id>` 用 `--page` 选集（`all`、`1,3-5`）。高画质或会员内容需先 `login`。

## Batch Input

`download` 支持合集和收藏夹链接，可配合 `--limit` 与 `--delay-per-page` 控制规模与请求间隔：

```bash
bilidown download "https://space.bilibili.com/123/favlist?fid=456" --limit 10 --delay-per-page 2
```

## Configuration

命令行参数优先于配置文件。

```bash
bilidown config path                      # 配置文件路径
bilidown config show                      # 查看当前配置
bilidown config set output_dir ./videos   # 设置默认输出目录
bilidown config set codec hevc,avc        # 设置默认编码偏好
bilidown config set connections 8         # 设置默认并发数
bilidown config unset codec               # 取消某项配置
```

## Archive

归档记录已完成的 `aid`、`cid` 与下载模式（`both` / `audio-only` / `video-only` 独立记录），配合 `--skip-archived` 避免重复下载。

```bash
bilidown archive list    # 查看归档
bilidown archive clear   # 清空归档
```

## Output Template

默认模板：

```text
{title}/P{page}-{part}-{quality}-{codec}.mp4
```

可用变量：`{title}`、`{page}`、`{part}`、`{quality}`、`{codec}`、`{bvid}`、`{aid}`、`{cid}`、`{owner}`

```bash
bilidown download BV1xxxxxxx --template "{owner}/{title}/P{page}-{part}-{cid}.mp4"
```

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Disclaimer

本项目与 Bilibili 无关，仅供个人学习与合法的资料备份用途。请遵守 Bilibili 服务条款、创作者权益及相关版权法律。
