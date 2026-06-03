use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::Context;

use crate::{
    archive::{Archive, ArchiveEntry, default_archive_path, read_archive, write_archive},
    assets::{AssetOptions, download_cover, download_danmaku, download_subtitles, fetch_subtitles},
    batch::{BatchInput, fetch_batch_videos, parse_batch_input},
    cli::{ArchiveCommands, ConfigCommands, DEFAULT_TEMPLATE},
    client::BiliClient,
    config::{self, AppConfig, ConfigKey},
    download::{DownloadConfig, download_stream_with_urls, sidecar_path},
    fs_utils::render_output_path,
    input::{VideoInput, parse_video_input},
    mux::mux_to_mp4,
    page::select_pages,
    video::{
        AudioQualityPreference, CodecPreference, QualityPreference, VideoInfo, VideoPage,
        fetch_play_info, fetch_video_info,
    },
};

pub async fn info(client: &BiliClient, raw_input: &str) -> anyhow::Result<()> {
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

pub struct DownloadOptions {
    page: String,
    quality: QualityPreference,
    codec: CodecPreference,
    audio_quality: AudioQualityPreference,
    out_dir: PathBuf,
    template: String,
    skip_mux: bool,
    ffmpeg_path: Option<PathBuf>,
    download_config: DownloadConfig,
    assets: AssetOptions,
    delay_per_page: u64,
    limit: Option<usize>,
    save_archive: bool,
    skip_archived: bool,
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
        let connections = if no_multi_thread {
            1
        } else {
            connections.or(cfg.connections).unwrap_or(8).max(1)
        };

        Ok(Self {
            page,
            quality: QualityPreference::parse(&quality)?,
            codec: CodecPreference::parse(&codec)?,
            audio_quality: AudioQualityPreference::parse(&audio_quality)?,
            out_dir,
            template,
            skip_mux,
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
            if opts.skip_archived && archive.contains(info.aid, page.cid) {
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
    let video = parsed.select_video(opts.quality, &opts.codec)?;
    let audio = parsed.select_audio(opts.audio_quality)?;

    let mut vars = BTreeMap::new();
    vars.insert("title", info.title.clone());
    vars.insert("page", page.index.to_string());
    vars.insert("part", page.title.clone());
    vars.insert("quality", video.quality_name.clone());
    vars.insert("codec", video.codec_name.clone());
    vars.insert("bvid", info.bvid.clone());
    vars.insert("aid", info.aid.to_string());
    vars.insert("cid", page.cid.to_string());
    vars.insert("owner", info.owner_name.clone());

    let mut output = render_output_path(&opts.out_dir, &opts.template, &vars);
    if output.extension().is_none() {
        output.set_extension("mp4");
    }

    let video_path = sidecar_path(&output, "video.m4s");
    let audio_path = sidecar_path(&output, "audio.m4s");

    println!("下载 P{}：{}", page.index, page.title);
    println!(
        "视频：{} {} {}kbps",
        video.quality_name,
        video.codec_name,
        video.bandwidth / 1000
    );
    println!("音频：{} {}kbps", audio.codec_name, audio.bandwidth / 1000);

    let mut video_urls = vec![video.base_url.clone()];
    video_urls.extend(video.backup_urls.clone());
    let mut audio_urls = vec![audio.base_url.clone()];
    audio_urls.extend(audio.backup_urls.clone());

    download_stream_with_urls(
        client,
        &video_urls,
        &video_path,
        "video",
        opts.download_config,
    )
    .await?;
    download_stream_with_urls(
        client,
        &audio_urls,
        &audio_path,
        "audio",
        opts.download_config,
    )
    .await?;

    if opts.assets.cover {
        let _ = download_cover(client, &info.cover_url, &output, opts.download_config).await?;
    }
    if opts.assets.subtitle {
        let subtitles = fetch_subtitles(client, info.aid, page.cid).await?;
        let paths = download_subtitles(client, &subtitles, &output).await?;
        if paths.is_empty() {
            println!("未找到字幕。");
        }
    }
    if opts.assets.danmaku {
        let _ = download_danmaku(client, page.cid, &output).await?;
    }

    if opts.skip_mux {
        println!("已跳过合并：");
        println!("  {}", video_path.display());
        println!("  {}", audio_path.display());
        return Ok(Some(archive_entry(info, page, &video, &audio, &output)));
    }

    mux_to_mp4(
        opts.ffmpeg_path.as_deref(),
        &video_path,
        &audio_path,
        &output,
    )
    .await?;
    let _ = tokio::fs::remove_file(&video_path).await;
    let _ = tokio::fs::remove_file(&audio_path).await;

    println!("下载完成：{}", output.display());
    Ok(Some(archive_entry(info, page, &video, &audio, &output)))
}

fn archive_entry(
    info: &VideoInfo,
    page: &VideoPage,
    video: &crate::video::VideoTrack,
    audio: &crate::video::AudioTrack,
    output: &std::path::Path,
) -> ArchiveEntry {
    ArchiveEntry {
        aid: info.aid,
        cid: page.cid,
        quality: video.quality_name.clone(),
        codec: video.codec_name.clone(),
        audio: audio.id.to_string(),
        output: output.display().to_string(),
        completed_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default(),
    }
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
