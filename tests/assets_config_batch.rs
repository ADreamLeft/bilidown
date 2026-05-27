use std::path::PathBuf;

use bilidown::{
    archive::{Archive, ArchiveEntry},
    assets::{bili_subtitle_json_to_srt, detect_image_extension},
    batch::{BatchInput, parse_batch_input},
    config::{AppConfig, ConfigKey},
};

#[test]
fn converts_bilibili_subtitle_json_to_srt() {
    let json = r#"{
      "body": [
        {"from": 0.0, "to": 1.25, "content": "你好"},
        {"from": 65.5, "to": 66.75, "content": "第二行\n换行"}
      ]
    }"#;

    let srt = bili_subtitle_json_to_srt(json).unwrap();

    assert_eq!(
        srt,
        "1\n00:00:00,000 --> 00:00:01,250\n你好\n\n2\n00:01:05,500 --> 00:01:06,750\n第二行 换行\n\n"
    );
}

#[test]
fn detects_image_extension_from_url() {
    assert_eq!(
        detect_image_extension("https://i0.hdslb.com/bfs/archive/foo.png@672w_378h_1c.webp"),
        "png"
    );
    assert_eq!(detect_image_extension("https://example.com/no-ext"), "jpg");
}

#[test]
fn parses_collection_and_favorite_inputs() {
    assert_eq!(
        parse_batch_input("https://space.bilibili.com/123/channel/collectiondetail?sid=456")
            .unwrap(),
        BatchInput::Collection { sid: 456 }
    );
    assert_eq!(
        parse_batch_input("https://space.bilibili.com/123/favlist?fid=789").unwrap(),
        BatchInput::Favorite {
            media_id: 789,
            owner_mid: Some(123)
        }
    );
}

#[test]
fn config_sets_known_keys() {
    let mut cfg = AppConfig::default();

    cfg.set(ConfigKey::OutputDir, "/tmp/videos").unwrap();
    cfg.set(ConfigKey::Codec, "hevc,avc").unwrap();
    cfg.set(ConfigKey::Connections, "16").unwrap();
    cfg.set(ConfigKey::Cover, "true").unwrap();

    assert_eq!(cfg.output_dir, Some(PathBuf::from("/tmp/videos")));
    assert_eq!(cfg.codec.as_deref(), Some("hevc,avc"));
    assert_eq!(cfg.connections, Some(16));
    assert_eq!(cfg.cover, Some(true));
}

#[test]
fn archive_records_and_detects_completed_items() {
    let mut archive = Archive::default();
    let entry = ArchiveEntry {
        aid: 1,
        cid: 2,
        quality: "1080P 高清".to_string(),
        codec: "AVC".to_string(),
        audio: "30280".to_string(),
        output: "out.mp4".to_string(),
        completed_at: 1_700_000_000,
    };

    assert!(!archive.contains(1, 2));
    archive.add(entry);
    assert!(archive.contains(1, 2));
}
