# bilidown

`bilidown` 是一个用 Rust 编写的 Bilibili Web DASH 视频下载命令行工具。它面向个人学习、资料备份和命令行自动化场景，默认通过 Web 端接口获取视频信息和音视频流，并使用 `ffmpeg` 合并输出文件。

## Features

- 扫码登录并复用本机登录态
- 查看视频分 P、清晰度、编码和音频流信息
- 下载普通视频、合集和收藏夹中的视频
- 支持分 P 选择：`1`、`1,3-5`、`all`
- 支持画质、编码和音频质量偏好
- 支持并发 Range 分片下载、断点续传、失败重试和备用 URL fallback
- 支持下载封面、字幕和弹幕
- 支持 `--audio-only` 和 `--video-only`
- 支持默认配置和下载归档，避免重复下载

## Requirements

- Rust stable
- `ffmpeg`

`ffmpeg` 需要在 `PATH` 中，或者在下载时通过 `--ffmpeg-path` 指定。

## Installation

从源码安装：

```bash
cargo install --path .
```

开发时直接运行：

```bash
cargo run -- --help
```

## Usage

扫码登录：

```bash
bilidown login
```

检查登录状态：

```bash
bilidown status
```

查看视频信息：

```bash
bilidown info BV1xxxxxxx
```

下载单个视频的第 1 个分 P：

```bash
bilidown download BV1xxxxxxx
```

下载全部分 P：

```bash
bilidown download BV1xxxxxxx --page all
```

指定清晰度、编码偏好和输出目录：

```bash
bilidown download BV1xxxxxxx --quality best --codec av1,hevc,avc -o ./videos
```

只下载音频：

```bash
bilidown download BV1xxxxxxx --audio-only
```

只下载视频：

```bash
bilidown download BV1xxxxxxx --video-only
```

同时下载封面、字幕和弹幕：

```bash
bilidown download BV1xxxxxxx --all-assets
```

下载并写入归档：

```bash
bilidown download BV1xxxxxxx --page all --save-archive --skip-archived
```

保留原始 `m4s` 文件，不执行合并：

```bash
bilidown download BV1xxxxxxx --skip-mux
```

查看完整参数：

```bash
bilidown download --help
```

## Batch Input

`download` 支持普通 BV/av/视频 URL，也支持部分合集和收藏夹链接。批量输入可以配合 `--limit` 和 `--delay-per-page` 控制下载规模和请求间隔：

```bash
bilidown download "https://space.bilibili.com/123/favlist?fid=456" --limit 10 --delay-per-page 2
```

## Configuration

查看配置文件路径：

```bash
bilidown config path
```

查看当前配置：

```bash
bilidown config show
```

设置默认输出目录：

```bash
bilidown config set output_dir ./videos
```

设置默认编码偏好：

```bash
bilidown config set codec hevc,avc
```

设置默认并发连接数：

```bash
bilidown config set connections 8
```

取消某个配置：

```bash
bilidown config unset codec
```

## Archive

下载归档记录已完成的 `aid`、`cid` 和下载模式。`both`、`audio-only`、`video-only` 会独立记录。

查看归档：

```bash
bilidown archive list
```

清空归档：

```bash
bilidown archive clear
```

## Output Template

默认输出模板为：

```text
{title}/P{page}-{part}-{quality}-{codec}.mp4
```

可用变量：

```text
{title} {page} {part} {quality} {codec} {bvid} {aid} {cid} {owner}
```

示例：

```bash
bilidown download BV1xxxxxxx --template "{owner}/{title}/P{page}-{part}-{cid}.mp4"
```

## Development

常用检查命令：

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Disclaimer

This project is not affiliated with Bilibili. Use it only for personal learning and lawful archival purposes. Respect Bilibili's terms of service, creators' rights, and applicable copyright laws.
