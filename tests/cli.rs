use bilidown::cli::{Cli, DEFAULT_TEMPLATE};
use clap::{CommandFactory, Parser};

#[test]
fn cli_is_exposed_from_library_and_keeps_download_defaults() {
    let mut command = Cli::command();
    let help = command.render_long_help().to_string();

    assert!(help.contains("download"));
    assert_eq!(
        DEFAULT_TEMPLATE,
        "{title}/P{page}-{part}-{quality}-{codec}.mp4"
    );

    let _cli = Cli::parse_from(["bilidown", "download", "BV1rp4y1e745"]);
}

#[test]
fn cli_accepts_audio_only_and_video_only_flags() {
    let mut command = Cli::command();
    let help = command
        .find_subcommand_mut("download")
        .expect("download subcommand")
        .render_long_help()
        .to_string();
    assert!(help.contains("--audio-only"));
    assert!(help.contains("--video-only"));

    Cli::try_parse_from(["bilidown", "download", "--audio-only", "BV1rp4y1e745"]).unwrap();
    Cli::try_parse_from(["bilidown", "download", "--video-only", "BV1rp4y1e745"]).unwrap();
}

#[test]
fn cli_rejects_combined_audio_only_and_video_only_flags() {
    let err = match Cli::try_parse_from([
        "bilidown",
        "download",
        "--audio-only",
        "--video-only",
        "BV1rp4y1e745",
    ]) {
        Ok(_) => panic!("audio-only and video-only should conflict"),
        Err(err) => err,
    };

    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}
