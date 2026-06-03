use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::{auth, client::BiliClient, commands, config};

pub const DEFAULT_TEMPLATE: &str = "{title}/P{page}-{part}-{quality}-{codec}.mp4";

#[derive(Parser)]
#[command(version, about = "Rust Bilibili Web video downloader")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
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
pub enum ConfigCommands {
    Show,
    Path,
    Set { key: String, value: String },
    Unset { key: String },
}

#[derive(Subcommand)]
pub enum ArchiveCommands {
    List,
    Clear,
}

pub async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let client = BiliClient::new()?;
    let cfg = config::read_app_config()?;

    match cli.command {
        Commands::Login => auth::login(&client).await?,
        Commands::Status => auth::status(&client).await?,
        Commands::Info { input } => commands::info(&client, &input).await?,
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
            let opts = commands::DownloadOptions::from_cli(
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
            commands::download(&client, &input, opts).await?;
        }
        Commands::Config { command } => commands::config(command)?,
        Commands::Archive { command } => commands::archive(command)?,
    }
    Ok(())
}
