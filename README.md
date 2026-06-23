# bilidown: A Command-line Bilibili Video Downloader 🎬

[![CI](https://github.com/ADreamLeft/bilidown/actions/workflows/ci.yml/badge.svg)](https://github.com/ADreamLeft/bilidown/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/ADreamLeft/bilidown)](./LICENSE)
[![Issues](https://img.shields.io/github/issues/ADreamLeft/bilidown)](https://github.com/ADreamLeft/bilidown/issues)
[![Stars](https://img.shields.io/github/stars/ADreamLeft/bilidown?style=social)](https://github.com/ADreamLeft/bilidown)

用 Rust 写的 Bilibili 命令行下载器：扫码登录、关键词搜索、并发分片下载、断点续传，支持普通视频 / 合集 / 收藏夹 / 番剧，自动用 `ffmpeg` 合并。单个二进制，面向个人学习、资料备份和命令行自动化。

> 觉得好用的话点个 ⭐ 呗，这是我持续更新的动力。

## Overview

- 🔑 扫码登录，复用本机登录态
- 🔍 搜索视频与 UP 主（多种排序、时长过滤）
- 📺 下载普通视频、合集、收藏夹
- 🎞️ 下载番剧 / 影视（`ep` / `ss`）
- 🎯 分 P / 剧集选择：`1`、`1,3-5`、`all`
- 🧩 画质、编码、音频质量偏好
- 🚀 并发 Range 分片下载、断点续传、失败重试、备用 URL fallback
- 📎 下载封面、字幕（转 SRT）、弹幕
- 🎵 `--audio-only` / 🎬 `--video-only`
- 🗂️ 默认配置 + 下载归档去重

## Demo 🎬

> 动图约 10 倍速。

**搜索 → 下载：**

![search and download](assets/demo-search.gif)

**番剧（EP / SS）：**

![bangumi](assets/demo-bangumi.gif)

## Getting Started 🚀

### [1/3] 安装 bilidown

从源码安装（需要 Rust stable，edition 2024）：

```bash
cargo install --path .
```

> [!TIP]
> 暂未发布到 crates.io；发布后即可直接 `cargo install bilidown`。

### [2/3] 安装 FFmpeg

bilidown 用 `ffmpeg` 合并音视频流，需要它在 `PATH` 中（或下载时用 `--ffmpeg-path` 指定）：

- 🍎 macOS: `brew install ffmpeg`
- 🪟 Windows: `winget install ffmpeg`
- 🐧 Linux: `sudo apt install ffmpeg`（或对应发行版的包管理器）

### [3/3] 登录

```bash
bilidown login     # 终端里显示二维码，用 Bilibili App 扫码确认
bilidown status    # 查看当前登录态
```

> [!TIP]
> 不登录也能下免费内容；高画质、大会员或番剧会员集需要先登录。

## 搜索 🔍

- 🔍 搜视频（综合排序）: `bilidown search 关键词`
- ▶️ 按播放量，取前 5 条: `bilidown search 关键词 --order play -n 5`
- 🆕 最新发布 + 时长 30–60min: `bilidown search 关键词 --order new --duration long`
- 👤 搜 UP 主，按粉丝数: `bilidown search 用户名 --type user --order fans`

排序：video 支持 `default` / `play` / `new` / `danmaku` / `favorite` / `comment`，user 支持 `default` / `fans` / `level`。搜出来的 BVID 可直接丢给 `download`。

## 下载 📥

- 📥 第 1P / 全部分 P: `bilidown download BV1xxxxxxx` / `bilidown download BV1xxxxxxx --page all`
- ⚙️ 指定画质、编码、目录: `bilidown download BV1xxxxxxx --quality 1080 --codec av1,hevc,avc -o ./videos`
- 🎞️ 番剧单集 / 整季 / 选集: `bilidown download ep374660` / `download ss33802 --page all` / `download ss33802 --page 1,3-5`
- 🎵 只下音频 / 只下视频: `bilidown download BV1xxxxxxx --audio-only` / `--video-only`
- 📎 封面 + 字幕 + 弹幕: `bilidown download BV1xxxxxxx --all-assets`
- 🗂️ 写入归档并跳过已下载: `bilidown download ss33802 --page all --save-archive --skip-archived`
- 📦 保留 m4s 不合并: `bilidown download BV1xxxxxxx --skip-mux`

合集 / 收藏夹链接可直接下，用 `--limit` 和 `--delay-per-page` 控制规模与请求间隔：

```bash
bilidown download "https://space.bilibili.com/123/favlist?fid=456" --limit 10 --delay-per-page 2
```

> [!TIP]
> 画质 `--quality` 支持友好写法：`best`（默认，最高可用）、`360` / `480` / `720` / `1080`，以及 `4k` / `8k` / `hdr` / `dolby`，同时兼容原始 qn 数字。高画质需登录、部分需大会员。

> [!TIP]
> 番剧：传 `ep<id>` 默认只下该集；传 `ss<id>` 用 `--page` 选集。完整参数见 `bilidown download --help`。

## 输出模板 🏷️

默认模板 `{title}/P{page}-{part}-{quality}-{codec}.mp4`，可用变量：`{title}` `{page}` `{part}` `{quality}` `{codec}` `{bvid}` `{aid}` `{cid}` `{owner}`

```bash
bilidown download BV1xxxxxxx --template "{owner}/{title}/P{page}-{part}-{cid}.mp4"
```

## 配置与归档 🗂️

配置（命令行参数优先于配置文件，`config path` 可看文件位置）：

```bash
bilidown config show                      # 查看当前配置
bilidown config set output_dir ./videos   # 默认输出目录
bilidown config set codec hevc,avc        # 默认编码偏好
bilidown config set connections 8         # 默认并发数
bilidown config unset codec               # 取消某项
```

归档记录已完成的 `aid` / `cid` 与下载模式（`both` / `audio-only` / `video-only` 各自独立），配合 `--skip-archived` 防止重复下载：

```bash
bilidown archive list     # 查看归档
bilidown archive clear    # 清空归档
```

## Motivation 💡

市面上的 B 站命令行下载器要么停更（BBDown 已归档、bili-cli-rs 三年没动），要么是带 GUI 的桌面应用。我想要一个**还在维护、纯 Rust、纯命令行**的版本：单个二进制、好脚本化、方便批量备份；顺手把 WBI 签名、并发分片、断点续传这些现代 Web 接口该有的东西都做扎实。

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Disclaimer

本项目与 Bilibili 无关，仅供个人学习与合法的资料备份用途。请遵守 Bilibili 服务条款、创作者权益及相关版权法律。
