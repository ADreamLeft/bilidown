use std::{collections::BTreeMap, path::PathBuf};

use anyhow::Context;
use bilidown::{
    auth,
    client::BiliClient,
    download::{download_stream, sidecar_path},
    fs_utils::render_output_path,
    input::parse_video_input,
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
    },
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
        } => {
            let opts = DownloadOptions {
                page,
                quality: QualityPreference::parse(&quality)?,
                codec: CodecPreference::parse(&codec)?,
                audio_quality: AudioQualityPreference::parse(&audio_quality)?,
                out_dir,
                template,
                skip_mux,
                ffmpeg_path,
            };
            command_download(&client, &input, opts).await?;
        }
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
}

async fn command_download(
    client: &BiliClient,
    raw_input: &str,
    opts: DownloadOptions,
) -> anyhow::Result<()> {
    let input = parse_video_input(raw_input)?;
    let info = fetch_video_info(client, &input).await?;
    let pages = select_pages(&opts.page, info.pages.len())?;

    for page_index in pages {
        let page = info
            .pages
            .iter()
            .find(|p| p.index == page_index)
            .with_context(|| format!("page {page_index} not found"))?;
        download_page(client, &info, page, &opts).await?;
    }

    Ok(())
}

async fn download_page(
    client: &BiliClient,
    info: &VideoInfo,
    page: &VideoPage,
    opts: &DownloadOptions,
) -> anyhow::Result<()> {
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

    download_stream(client, &video.base_url, &video_path, "video").await?;
    download_stream(client, &audio.base_url, &audio_path, "audio").await?;

    if opts.skip_mux {
        println!("已跳过合并：");
        println!("  {}", video_path.display());
        println!("  {}", audio_path.display());
        return Ok(());
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
    Ok(())
}
