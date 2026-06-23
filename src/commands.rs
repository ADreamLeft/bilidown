use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::Context;

use crate::{
    archive::{
        Archive, ArchiveEntry, ArchiveMode, default_archive_path, read_archive, write_archive,
    },
    assets::{AssetOptions, download_cover, download_danmaku, download_subtitles, fetch_subtitles},
    bangumi::{BangumiInput, Episode, Season, fetch_season, parse_bangumi_input},
    batch::{BatchInput, fetch_batch_videos, parse_batch_input},
    cli::{ArchiveCommands, ConfigCommands, DEFAULT_TEMPLATE},
    client::BiliClient,
    config::{self, AppConfig, ConfigKey},
    download::{DownloadConfig, download_stream_with_urls, sidecar_path},
    fs_utils::render_output_path,
    input::{VideoInput, parse_video_input},
    mux::{mux_single_stream, mux_to_mp4},
    page::select_pages,
    video::{
        AudioQualityPreference, AudioTrack, CodecPreference, ParsedPlay, QualityPreference,
        VideoInfo, VideoPage, VideoTrack, fetch_pgc_play_info, fetch_play_info, fetch_video_info,
    },
};

pub async fn info(client: &BiliClient, raw_input: &str) -> anyhow::Result<()> {
    if let Some(bangumi) = parse_bangumi_input(raw_input) {
        return info_bangumi(client, &bangumi).await;
    }
    let input = parse_video_input(raw_input)?;
    let info = fetch_video_info(client, &input).await?;

    println!("{} ({})", info.title, info.bvid);
    println!("UP: {} ({})", info.owner_name, info.owner_mid);
    println!("AID: {}", info.aid);
    println!();
    println!("分 P:");
    for page in &info.pages {
        println!(
            "  P{} {}  cid={}  {}s",
            page.index, page.title, page.cid, page.duration
        );
    }

    let Some(first_page) = info.pages.first() else {
        return Ok(());
    };
    let parsed = fetch_play_info(client, info.aid, first_page.cid, QualityPreference::Best).await?;
    println!();
    println!("P{} 可用视频流:", first_page.index);
    for track in &parsed.video_tracks {
        println!(
            "  qn={} {} {} {} {}kbps",
            track.quality_id,
            track.quality_name,
            track
                .width
                .zip(track.height)
                .map(|(w, h)| format!("{w}x{h}"))
                .unwrap_or_else(|| "-".to_string()),
            track.codec_name,
            track.bandwidth / 1000
        );
    }
    println!("可用音频流:");
    for track in &parsed.audio_tracks {
        println!(
            "  id={} {} {}kbps",
            track.id,
            track.codec_name,
            track.bandwidth / 1000
        );
    }

    Ok(())
}

async fn info_bangumi(client: &BiliClient, input: &BangumiInput) -> anyhow::Result<()> {
    let season = fetch_season(client, input).await?;
    println!("{}", season.title);
    println!("共 {} 集", season.episodes.len());
    println!();
    for ep in &season.episodes {
        let extra = if ep.long_title.trim().is_empty() {
            String::new()
        } else {
            format!("  {}", ep.long_title)
        };
        println!(
            "  EP{} {}{}  ep_id={} cid={}",
            ep.index, ep.title, extra, ep.ep_id, ep.cid
        );
    }
    Ok(())
}

pub struct DownloadOptions {
    page: String,
    quality: QualityPreference,
    codec: CodecPreference,
    audio_quality: AudioQualityPreference,
    out_dir: PathBuf,
    template: String,
    template_is_default: bool,
    skip_mux: bool,
    mode: DownloadMode,
    ffmpeg_path: Option<PathBuf>,
    download_config: DownloadConfig,
    assets: AssetOptions,
    delay_per_page: u64,
    limit: Option<usize>,
    save_archive: bool,
    skip_archived: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadMode {
    Both,
    Audio,
    Video,
}

impl DownloadMode {
    fn archive_mode(self) -> ArchiveMode {
        match self {
            Self::Both => ArchiveMode::Both,
            Self::Audio => ArchiveMode::Audio,
            Self::Video => ArchiveMode::Video,
        }
    }

    fn default_extension(self) -> &'static str {
        default_output_extension(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedDownloadTracks {
    pub video: Option<VideoTrack>,
    pub audio: Option<AudioTrack>,
}

pub fn default_output_extension(mode: DownloadMode) -> &'static str {
    match mode {
        DownloadMode::Both | DownloadMode::Video => "mp4",
        DownloadMode::Audio => "m4a",
    }
}

impl DownloadOptions {
    #[allow(clippy::too_many_arguments)]
    pub fn from_cli(
        cfg: &AppConfig,
        page: String,
        quality: String,
        codec: String,
        audio_quality: String,
        out_dir: PathBuf,
        template: String,
        skip_mux: bool,
        audio_only: bool,
        video_only: bool,
        ffmpeg_path: Option<PathBuf>,
        connections: Option<usize>,
        retries: Option<usize>,
        no_multi_thread: bool,
        cover: bool,
        subtitle: bool,
        danmaku: bool,
        all_assets: bool,
        delay_per_page: u64,
        limit: Option<usize>,
        save_archive: bool,
        skip_archived: bool,
    ) -> anyhow::Result<Self> {
        let quality = if quality == "best" {
            cfg.quality.as_deref().unwrap_or(&quality).to_string()
        } else {
            quality
        };
        let codec = if codec == "av1,hevc,avc" {
            cfg.codec.as_deref().unwrap_or(&codec).to_string()
        } else {
            codec
        };
        let audio_quality = if audio_quality == "best" {
            cfg.audio_quality
                .as_deref()
                .unwrap_or(&audio_quality)
                .to_string()
        } else {
            audio_quality
        };
        let out_dir = if out_dir == Path::new(".") {
            cfg.output_dir.clone().unwrap_or(out_dir)
        } else {
            out_dir
        };
        let template = if template == DEFAULT_TEMPLATE {
            cfg.template.clone().unwrap_or(template)
        } else {
            template
        };
        let template_is_default = template == DEFAULT_TEMPLATE;
        let connections = if no_multi_thread {
            1
        } else {
            connections.or(cfg.connections).unwrap_or(8).max(1)
        };
        let mode = match (audio_only, video_only) {
            (true, false) => DownloadMode::Audio,
            (false, true) => DownloadMode::Video,
            _ => DownloadMode::Both,
        };

        Ok(Self {
            page,
            quality: QualityPreference::parse(&quality)?,
            codec: CodecPreference::parse(&codec)?,
            audio_quality: AudioQualityPreference::parse(&audio_quality)?,
            out_dir,
            template,
            template_is_default,
            skip_mux,
            mode,
            ffmpeg_path,
            download_config: DownloadConfig {
                connections,
                retries: retries.or(cfg.retries).unwrap_or(3).max(1),
            },
            assets: AssetOptions {
                cover: all_assets || cover || cfg.cover.unwrap_or(false),
                subtitle: all_assets || subtitle || cfg.subtitle.unwrap_or(false),
                danmaku: all_assets || danmaku || cfg.danmaku.unwrap_or(false),
                embed_subtitle: false,
            },
            delay_per_page,
            limit,
            save_archive: save_archive || cfg.save_archive.unwrap_or(false),
            skip_archived: skip_archived || cfg.skip_archived.unwrap_or(false),
        })
    }
}

pub async fn download(
    client: &BiliClient,
    raw_input: &str,
    opts: DownloadOptions,
) -> anyhow::Result<()> {
    if let Some(bangumi) = parse_bangumi_input(raw_input) {
        return download_bangumi(client, &bangumi, &opts).await;
    }
    let batch_input = parse_batch_input(raw_input)?;
    let videos = fetch_batch_videos(client, &batch_input).await?;
    let videos = if let Some(limit) = opts.limit {
        videos.into_iter().take(limit).collect::<Vec<_>>()
    } else {
        videos
    };
    let archive_path = default_archive_path()?;
    let mut archive = read_archive(&archive_path)?;

    for (video_idx, video) in videos.iter().enumerate() {
        let input = VideoInput::Bvid(video.bvid.clone());
        let info = fetch_video_info(client, &input).await?;
        let pages = select_pages(&opts.page, info.pages.len())?;

        for page_index in pages {
            let page = info
                .pages
                .iter()
                .find(|p| p.index == page_index)
                .with_context(|| format!("page {page_index} not found"))?;
            if opts.skip_archived && archive.contains(info.aid, page.cid, opts.mode.archive_mode())
            {
                println!("跳过已归档：{} P{}", info.title, page.index);
                continue;
            }
            if let Some(entry) = download_page(client, &info, page, &opts).await?
                && opts.save_archive
            {
                archive.add(entry);
                write_archive(&archive_path, &archive)?;
            }
        }

        if opts.delay_per_page > 0 && video_idx + 1 < videos.len() {
            tokio::time::sleep(std::time::Duration::from_secs(opts.delay_per_page)).await;
        }

        if matches!(batch_input, BatchInput::Single(_)) {
            break;
        }
    }

    Ok(())
}

async fn download_page(
    client: &BiliClient,
    info: &VideoInfo,
    page: &VideoPage,
    opts: &DownloadOptions,
) -> anyhow::Result<Option<ArchiveEntry>> {
    let parsed = fetch_play_info(client, info.aid, page.cid, opts.quality).await?;
    let source = StreamSource {
        parsed,
        label: format!("P{}", page.index),
        title: info.title.clone(),
        page_var: page.index.to_string(),
        part: page.title.clone(),
        bvid: info.bvid.clone(),
        aid: info.aid,
        cid: page.cid,
        owner: info.owner_name.clone(),
        cover_url: info.cover_url.clone(),
    };
    download_source(client, &source, opts).await
}

async fn download_bangumi(
    client: &BiliClient,
    input: &BangumiInput,
    opts: &DownloadOptions,
) -> anyhow::Result<()> {
    let season = fetch_season(client, input).await?;
    anyhow::ensure!(!season.episodes.is_empty(), "番剧没有可下载的剧集");
    // 指定具体某集（ep）且未自定义 --page 时，默认只下这一集；
    // 其余情况（ss 整季，或显式 --page）按 --page 选择剧集序号。
    let selected = match input {
        BangumiInput::Ep(ep_id) if opts.page == "1" => {
            let index = season
                .episodes
                .iter()
                .find(|episode| episode.ep_id == *ep_id)
                .map(|episode| episode.index)
                .with_context(|| format!("ep_id {ep_id} 不在该季剧集列表中"))?;
            vec![index]
        }
        _ => select_pages(&opts.page, season.episodes.len())?,
    };
    let selected = if let Some(limit) = opts.limit {
        selected.into_iter().take(limit).collect::<Vec<_>>()
    } else {
        selected
    };

    let archive_path = default_archive_path()?;
    let mut archive = read_archive(&archive_path)?;

    let total = selected.len();
    for (done, ep_index) in selected.into_iter().enumerate() {
        let episode = season
            .episodes
            .iter()
            .find(|ep| ep.index == ep_index)
            .with_context(|| format!("episode {ep_index} not found"))?;
        if opts.skip_archived
            && archive.contains(episode.aid, episode.cid, opts.mode.archive_mode())
        {
            println!("跳过已归档：{} EP{}", season.title, episode.index);
            continue;
        }
        if let Some(entry) = download_episode(client, &season, episode, opts).await?
            && opts.save_archive
        {
            archive.add(entry);
            write_archive(&archive_path, &archive)?;
        }
        if opts.delay_per_page > 0 && done + 1 < total {
            tokio::time::sleep(std::time::Duration::from_secs(opts.delay_per_page)).await;
        }
    }

    Ok(())
}

async fn download_episode(
    client: &BiliClient,
    season: &Season,
    episode: &Episode,
    opts: &DownloadOptions,
) -> anyhow::Result<Option<ArchiveEntry>> {
    let parsed = fetch_pgc_play_info(
        client,
        episode.ep_id,
        episode.aid,
        episode.cid,
        opts.quality,
    )
    .await?;
    let part = if episode.long_title.trim().is_empty() {
        episode.title.clone()
    } else {
        format!("{} {}", episode.title, episode.long_title)
    };
    let source = StreamSource {
        parsed,
        label: format!("EP{}", episode.index),
        title: season.title.clone(),
        page_var: episode.index.to_string(),
        part,
        bvid: episode.bvid.clone(),
        aid: episode.aid,
        cid: episode.cid,
        owner: season.title.clone(),
        cover_url: episode.cover.clone(),
    };
    download_source(client, &source, opts).await
}

/// 一个待下载的流：来自普通投稿的分 P，或番剧的单集。
/// 已解析出可用音视频流，并带上渲染输出路径所需的模板变量。
struct StreamSource {
    parsed: ParsedPlay,
    /// 进度展示前缀，如 `P1` / `EP1`
    label: String,
    title: String,
    page_var: String,
    part: String,
    bvid: String,
    aid: u64,
    cid: u64,
    owner: String,
    cover_url: String,
}

async fn download_source(
    client: &BiliClient,
    source: &StreamSource,
    opts: &DownloadOptions,
) -> anyhow::Result<Option<ArchiveEntry>> {
    let tracks = select_download_tracks(
        &source.parsed,
        opts.mode,
        opts.quality,
        &opts.codec,
        opts.audio_quality,
    )?;

    let mut vars = BTreeMap::new();
    vars.insert("title", source.title.clone());
    vars.insert("page", source.page_var.clone());
    vars.insert("part", source.part.clone());
    vars.insert(
        "quality",
        tracks
            .video
            .as_ref()
            .map(|video| video.quality_name.clone())
            .unwrap_or_else(|| "audio".to_string()),
    );
    vars.insert(
        "codec",
        tracks
            .video
            .as_ref()
            .map(|video| video.codec_name.clone())
            .or_else(|| tracks.audio.as_ref().map(|audio| audio.codec_name.clone()))
            .unwrap_or_else(|| "unknown".to_string()),
    );
    vars.insert("bvid", source.bvid.clone());
    vars.insert("aid", source.aid.to_string());
    vars.insert("cid", source.cid.to_string());
    vars.insert("owner", source.owner.clone());

    let mut output = render_output_path(&opts.out_dir, &opts.template, &vars);
    if output.extension().is_none() || opts.template_is_default {
        output.set_extension(opts.mode.default_extension());
    }

    let video_path = sidecar_path(&output, "video.m4s");
    let audio_path = sidecar_path(&output, "audio.m4s");

    println!("下载 {}：{}", source.label, source.part);
    if let Some(video) = &tracks.video {
        println!(
            "视频：{} {} {}kbps",
            video.quality_name,
            video.codec_name,
            video.bandwidth / 1000
        );
    }
    if let Some(audio) = &tracks.audio {
        println!("音频：{} {}kbps", audio.codec_name, audio.bandwidth / 1000);
    }

    if let Some(video) = &tracks.video {
        let mut video_urls = vec![video.base_url.clone()];
        video_urls.extend(video.backup_urls.clone());
        download_stream_with_urls(
            client,
            &video_urls,
            &video_path,
            "video",
            opts.download_config,
        )
        .await?;
    }
    if let Some(audio) = &tracks.audio {
        let mut audio_urls = vec![audio.base_url.clone()];
        audio_urls.extend(audio.backup_urls.clone());
        download_stream_with_urls(
            client,
            &audio_urls,
            &audio_path,
            "audio",
            opts.download_config,
        )
        .await?;
    }

    if opts.assets.cover {
        let _ = download_cover(client, &source.cover_url, &output, opts.download_config).await?;
    }
    if opts.assets.subtitle {
        let subtitles = fetch_subtitles(client, source.aid, source.cid).await?;
        let paths = download_subtitles(client, &subtitles, &output).await?;
        if paths.is_empty() {
            println!("未找到字幕。");
        }
    }
    if opts.assets.danmaku {
        let _ = download_danmaku(client, source.cid, &output).await?;
    }

    if opts.skip_mux {
        println!("已跳过合并：");
        if tracks.video.is_some() {
            println!("  {}", video_path.display());
        }
        if tracks.audio.is_some() {
            println!("  {}", audio_path.display());
        }
        return Ok(Some(archive_entry(
            source.aid, source.cid, &tracks, opts.mode, &output,
        )));
    }

    match opts.mode {
        DownloadMode::Both => {
            mux_to_mp4(
                opts.ffmpeg_path.as_deref(),
                &video_path,
                &audio_path,
                &output,
            )
            .await?;
            let _ = tokio::fs::remove_file(&video_path).await;
            let _ = tokio::fs::remove_file(&audio_path).await;
        }
        DownloadMode::Audio => {
            mux_single_stream(opts.ffmpeg_path.as_deref(), &audio_path, &output).await?;
            let _ = tokio::fs::remove_file(&audio_path).await;
        }
        DownloadMode::Video => {
            mux_single_stream(opts.ffmpeg_path.as_deref(), &video_path, &output).await?;
            let _ = tokio::fs::remove_file(&video_path).await;
        }
    }

    println!("下载完成：{}", output.display());
    Ok(Some(archive_entry(
        source.aid, source.cid, &tracks, opts.mode, &output,
    )))
}

fn archive_entry(
    aid: u64,
    cid: u64,
    tracks: &SelectedDownloadTracks,
    mode: DownloadMode,
    output: &std::path::Path,
) -> ArchiveEntry {
    ArchiveEntry {
        aid,
        cid,
        mode: mode.archive_mode(),
        quality: tracks
            .video
            .as_ref()
            .map(|video| video.quality_name.clone())
            .unwrap_or_else(|| "audio".to_string()),
        codec: tracks
            .video
            .as_ref()
            .map(|video| video.codec_name.clone())
            .or_else(|| tracks.audio.as_ref().map(|audio| audio.codec_name.clone()))
            .unwrap_or_else(|| "unknown".to_string()),
        audio: tracks
            .audio
            .as_ref()
            .map(|audio| audio.id.to_string())
            .unwrap_or_default(),
        output: output.display().to_string(),
        completed_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default(),
    }
}

pub fn select_download_tracks(
    parsed: &ParsedPlay,
    mode: DownloadMode,
    quality: QualityPreference,
    codec: &CodecPreference,
    audio_quality: AudioQualityPreference,
) -> anyhow::Result<SelectedDownloadTracks> {
    Ok(match mode {
        DownloadMode::Both => SelectedDownloadTracks {
            video: Some(parsed.select_video(quality, codec)?),
            audio: Some(parsed.select_audio(audio_quality)?),
        },
        DownloadMode::Audio => SelectedDownloadTracks {
            video: None,
            audio: Some(parsed.select_audio(audio_quality)?),
        },
        DownloadMode::Video => SelectedDownloadTracks {
            video: Some(parsed.select_video(quality, codec)?),
            audio: None,
        },
    })
}

pub fn config(command: ConfigCommands) -> anyhow::Result<()> {
    match command {
        ConfigCommands::Show => {
            let cfg = config::read_app_config()?;
            println!("{}", toml::to_string_pretty(&cfg)?);
        }
        ConfigCommands::Path => println!("{}", config::config_path()?.display()),
        ConfigCommands::Set { key, value } => {
            let mut cfg = config::read_app_config()?;
            cfg.set(ConfigKey::from_str(&key)?, &value)?;
            config::write_app_config(&cfg)?;
        }
        ConfigCommands::Unset { key } => {
            let mut cfg = config::read_app_config()?;
            cfg.unset(ConfigKey::from_str(&key)?);
            config::write_app_config(&cfg)?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn search(
    client: &BiliClient,
    keyword: &str,
    search_type: crate::search::SearchType,
    order: &str,
    duration: &str,
    page: u32,
    page_size: u32,
    limit: Option<usize>,
) -> anyhow::Result<()> {
    use crate::search::{SearchParams, SearchResult};

    let order = crate::search::resolve_order(search_type, order)?;
    let duration = crate::search::resolve_duration(duration)?;
    let params = SearchParams {
        keyword,
        search_type,
        order,
        duration,
        page,
        page_size,
    };

    let mut results = crate::search::search(client, &params).await?;
    if let Some(limit) = limit {
        results.truncate(limit);
    }
    if results.is_empty() {
        println!("没有找到结果。");
        return Ok(());
    }

    for (idx, result) in results.iter().enumerate() {
        let n = idx + 1;
        match result {
            SearchResult::Video(video) => {
                println!("{n:>2}. {}  [{}]", video.title, video.duration);
                println!(
                    "    {}  UP:{}  ▶{}  弹幕{}  {}  {}",
                    video.bvid,
                    video.author,
                    humanize_count(video.play),
                    humanize_count(video.danmaku),
                    video.typename,
                    format_date(video.pubdate),
                );
            }
            SearchResult::User(user) => {
                let sign = if user.sign.trim().is_empty() {
                    "-"
                } else {
                    user.sign.trim()
                };
                println!("{n:>2}. {}  (Lv{})", user.uname, user.level);
                println!(
                    "    UID:{}  粉丝:{}  投稿:{}  {}",
                    user.mid,
                    humanize_count(user.fans),
                    user.videos,
                    sign,
                );
            }
        }
    }

    Ok(())
}

/// 把计数转成更易读的 万/亿 形式
fn humanize_count(n: i64) -> String {
    if n < 0 {
        return "-".to_string();
    }
    if n >= 100_000_000 {
        format!("{:.1}亿", n as f64 / 100_000_000.0)
    } else if n >= 10_000 {
        format!("{:.1}万", n as f64 / 10_000.0)
    } else {
        n.to_string()
    }
}

/// 把 unix 秒按 UTC+8 转成 YYYY-MM-DD（civil_from_days 算法，避免引入日期库）
fn format_date(unix: i64) -> String {
    if unix <= 0 {
        return "-".to_string();
    }
    let days = (unix + 8 * 3600).div_euclid(86400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    format!("{year:04}-{month:02}-{day:02}")
}

pub fn archive(command: ArchiveCommands) -> anyhow::Result<()> {
    let path = default_archive_path()?;
    match command {
        ArchiveCommands::List => {
            let archive = read_archive(&path)?;
            for entry in archive.entries {
                println!(
                    "{} {} {} {} {}",
                    entry.aid, entry.cid, entry.quality, entry.codec, entry.output
                );
            }
        }
        ArchiveCommands::Clear => {
            write_archive(&path, &Archive::default())?;
            println!("归档已清空。");
        }
    }
    Ok(())
}
