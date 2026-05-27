use std::{collections::BTreeMap, path::PathBuf, str::FromStr};

use anyhow::Context;
use bilidown::{
    archive::{Archive, ArchiveEntry, default_archive_path, read_archive, write_archive},
    assets::{AssetOptions, download_cover, download_danmaku, download_subtitles, fetch_subtitles},
    auth,
    batch::{BatchInput, fetch_batch_videos, parse_batch_input},
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
use clap::{Parser, Subcommand};

const DEFAULT_TEMPLATE: &str = "{title}/P{page}-{part}-{quality}-{codec}.mp4";

#[derive(Parser)]
#[command(version, about = "Rust Bilibili Web video downloader")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 使用 Bilibili 客户端扫码登录 Web 账号
    Login,
    /// 检查当前保存的登录态
    Status,
    /// 显示视频分 P 和可用 DASH 音视频流
    Info {
        /// BV/av/普通视频 URL
        input: String,
    },
    /// 下载普通视频 Web DASH 音视频流并用 ffmpeg 合并
    Download {
        /// BV/av/普通视频 URL
        input: String,
        /// 分 P 选择，例如 1、1,3-5、all
        #[arg(short, long, default_value = "1")]
        page: String,
        /// 清晰度：best 或 qn 数字，例如 80、112
        #[arg(long, default_value = "best")]
        quality: String,
        /// 编码优先级，例如 av1,hevc,avc
        #[arg(long, default_value = "av1,hevc,avc")]
        codec: String,
        /// 音频流：best 或音频 id，例如 30280
        #[arg(long = "audio-quality", default_value = "best")]
        audio_quality: String,
        /// 输出目录
        #[arg(short = 'o', long, default_value = ".")]
        out_dir: PathBuf,
        /// 输出模板，可用 {title} {page} {part} {quality} {codec} {bvid} {aid} {cid} {owner}
        #[arg(long, default_value = DEFAULT_TEMPLATE)]
        template: String,
        /// 只下载 video/audio m4s 文件，不执行 ffmpeg 合并
        #[arg(long, default_value_t = false)]
        skip_mux: bool,
        /// 指定 ffmpeg 路径
        #[arg(long)]
        ffmpeg_path: Option<PathBuf>,
        /// 并发 Range 分片连接数
        #[arg(long)]
        connections: Option<usize>,
        /// 下载失败重试次数
        #[arg(long)]
        retries: Option<usize>,
        /// 禁用多线程分片下载
        #[arg(long, default_value_t = false)]
        no_multi_thread: bool,
        /// 下载封面
        #[arg(long, default_value_t = false)]
        cover: bool,
        /// 下载字幕并转换为 srt
        #[arg(long, default_value_t = false)]
        subtitle: bool,
        /// 下载弹幕 XML
        #[arg(long, default_value_t = false)]
        danmaku: bool,
        /// 下载封面、字幕、弹幕
        #[arg(long, default_value_t = false)]
        all_assets: bool,
        /// 多任务之间的延迟秒数
        #[arg(long, default_value_t = 0)]
        delay_per_page: u64,
        /// 批量输入最多下载多少个视频
        #[arg(long)]
        limit: Option<usize>,
        /// 下载完成后写入归档
        #[arg(long, default_value_t = false)]
        save_archive: bool,
        /// 已在归档中的 aid/cid 跳过
        #[arg(long, default_value_t = false)]
        skip_archived: bool,
    },
    /// 显示或修改默认配置
    #[command(arg_required_else_help(true))]
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
    /// 查看或清空已下载归档
    #[command(arg_required_else_help(true))]
    Archive {
        #[command(subcommand)]
        command: ArchiveCommands,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    Show,
    Path,
    Set { key: String, value: String },
    Unset { key: String },
}

#[derive(Subcommand)]
enum ArchiveCommands {
    List,
    Clear,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    if let Err(err) = run().await {
        eprintln!("Error: {err:#}");
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let client = BiliClient::new()?;
    let cfg = config::read_app_config()?;

    match cli.command {
        Commands::Login => auth::login(&client).await?,
        Commands::Status => auth::status(&client).await?,
        Commands::Info { input } => command_info(&client, &input).await?,
        Commands::Download {
            input,
            page,
            quality,
            codec,
            audio_quality,
            out_dir,
            template,
            skip_mux,
            ffmpeg_path,
            connections,
            retries,
            no_multi_thread,
            cover,
            subtitle,
            danmaku,
            all_assets,
            delay_per_page,
            limit,
            save_archive,
            skip_archived,
        } => {
            let opts = DownloadOptions::from_cli(
                &cfg,
                page,
                quality,
                codec,
                audio_quality,
                out_dir,
                template,
                skip_mux,
                ffmpeg_path,
                connections,
                retries,
                no_multi_thread,
                cover,
                subtitle,
                danmaku,
                all_assets,
                delay_per_page,
                limit,
                save_archive,
                skip_archived,
            )?;
            command_download(&client, &input, opts).await?;
        }
        Commands::Config { command } => command_config(command)?,
        Commands::Archive { command } => command_archive(command)?,
    }
    Ok(())
}

async fn command_info(client: &BiliClient, raw_input: &str) -> anyhow::Result<()> {
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

struct DownloadOptions {
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
    fn from_cli(
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
        let out_dir = if out_dir == PathBuf::from(".") {
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

async fn command_download(
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
    video: &bilidown::video::VideoTrack,
    audio: &bilidown::video::AudioTrack,
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

fn command_config(command: ConfigCommands) -> anyhow::Result<()> {
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

fn command_archive(command: ArchiveCommands) -> anyhow::Result<()> {
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
