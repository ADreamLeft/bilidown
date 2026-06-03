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
