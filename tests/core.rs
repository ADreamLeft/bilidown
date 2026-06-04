use std::collections::BTreeMap;

use bilidown::{
    commands::{DownloadMode, default_output_extension, select_download_tracks},
    fs_utils::safe_path_component,
    input::{VideoInput, parse_video_input},
    page::select_pages,
    video::{
        AudioQualityPreference, AudioTrack, CodecPreference, ParsedPlay, QualityPreference,
        VideoTrack, parse_play_response,
    },
    wbi::{mixin_key, sign_params_at},
};

const DASH_FIXTURE: &str = include_str!("fixtures/playurl_dash.json");

#[test]
fn parses_bvid_av_and_url_inputs() {
    assert_eq!(
        parse_video_input("BV1rp4y1e745").unwrap(),
        VideoInput::Bvid("BV1rp4y1e745".to_string())
    );
    assert_eq!(
        parse_video_input("av969628065").unwrap(),
        VideoInput::Aid(969_628_065)
    );
    assert_eq!(
        parse_video_input("https://www.bilibili.com/video/BV1rp4y1e745?p=2").unwrap(),
        VideoInput::Bvid("BV1rp4y1e745".to_string())
    );
}

#[test]
fn selects_page_expressions() {
    assert_eq!(select_pages("1", 5).unwrap(), vec![1]);
    assert_eq!(select_pages("1,3-5", 5).unwrap(), vec![1, 3, 4, 5]);
    assert_eq!(select_pages("all", 3).unwrap(), vec![1, 2, 3]);
}

#[test]
fn sanitizes_file_name_components() {
    assert_eq!(
        safe_path_component("a/b:c*?\"<>|  "),
        "a_b_c______".to_string()
    );
}

#[test]
fn signs_wbi_params_with_known_vector() {
    let key = mixin_key(
        "7cd084941338484aae1ad9425b84077c",
        "4932caff0ff746eab6f01bf08b70ac45",
    );
    assert_eq!(key, "ea1db124af3c7062474693fa704f4ff8");

    let mut params = BTreeMap::new();
    params.insert("foo".to_string(), "114".to_string());
    params.insert("bar".to_string(), "514".to_string());
    params.insert("zab".to_string(), "1919810".to_string());

    let signed = sign_params_at(params, &key, 1_702_204_169);
    assert_eq!(
        signed.to_query_string(),
        "bar=514&foo=114&wts=1702204169&zab=1919810&w_rid=8f6f2b5b3d485fe1886cec6a0be8c5d4"
    );
}

#[test]
fn parses_dash_tracks_and_selects_best_streams() {
    let parsed = parse_play_response(DASH_FIXTURE).unwrap();
    assert_eq!(parsed.video_tracks.len(), 3);
    assert_eq!(parsed.audio_tracks.len(), 3);

    let video = parsed
        .select_video(
            QualityPreference::Best,
            &CodecPreference::parse("av1,hevc,avc").unwrap(),
        )
        .unwrap();
    assert_eq!(video.quality_id, 80);
    assert_eq!(video.codec_name, "AV1");

    let audio = parsed
        .select_audio(AudioQualityPreference::Best)
        .expect("best audio");
    assert_eq!(audio.id, 30280);
}

#[test]
fn download_modes_select_only_required_streams() {
    let parsed = parse_play_response(DASH_FIXTURE).unwrap();
    let codec = CodecPreference::parse("av1,hevc,avc").unwrap();

    let both = select_download_tracks(
        &parsed,
        DownloadMode::Both,
        QualityPreference::Best,
        &codec,
        AudioQualityPreference::Best,
    )
    .unwrap();
    assert!(both.video.is_some());
    assert!(both.audio.is_some());

    let audio = select_download_tracks(
        &parsed,
        DownloadMode::Audio,
        QualityPreference::Best,
        &codec,
        AudioQualityPreference::Best,
    )
    .unwrap();
    assert!(audio.video.is_none());
    assert!(audio.audio.is_some());

    let video = select_download_tracks(
        &parsed,
        DownloadMode::Video,
        QualityPreference::Best,
        &codec,
        AudioQualityPreference::Best,
    )
    .unwrap();
    assert!(video.video.is_some());
    assert!(video.audio.is_none());
}

#[test]
fn download_modes_choose_expected_default_extensions() {
    assert_eq!(default_output_extension(DownloadMode::Both), "mp4");
    assert_eq!(default_output_extension(DownloadMode::Audio), "m4a");
    assert_eq!(default_output_extension(DownloadMode::Video), "mp4");
}

#[test]
fn single_stream_modes_do_not_require_the_other_stream_type() {
    let codec = CodecPreference::parse("av1,hevc,avc").unwrap();
    let audio_only_play = ParsedPlay {
        duration: Some(1),
        video_tracks: Vec::new(),
        audio_tracks: vec![AudioTrack {
            id: 30280,
            base_url: "https://audio.example/audio.m4s".to_string(),
            backup_urls: Vec::new(),
            codec_name: "M4A".to_string(),
            bandwidth: 128_000,
            size: None,
        }],
    };
    let video_only_play = ParsedPlay {
        duration: Some(1),
        video_tracks: vec![VideoTrack {
            quality_id: 80,
            quality_name: "1080P 高清".to_string(),
            base_url: "https://video.example/video.m4s".to_string(),
            backup_urls: Vec::new(),
            codec_name: "AVC".to_string(),
            codec_id: Some(7),
            bandwidth: 1_000_000,
            width: Some(1920),
            height: Some(1080),
            frame_rate: Some("30".to_string()),
            size: None,
        }],
        audio_tracks: Vec::new(),
    };

    assert!(
        select_download_tracks(
            &audio_only_play,
            DownloadMode::Audio,
            QualityPreference::Best,
            &codec,
            AudioQualityPreference::Best,
        )
        .is_ok()
    );
    assert!(
        select_download_tracks(
            &video_only_play,
            DownloadMode::Video,
            QualityPreference::Best,
            &codec,
            AudioQualityPreference::Best,
        )
        .is_ok()
    );
}

#[test]
fn parses_dash_tracks_when_bilibili_returns_duplicate_camel_and_snake_fields() {
    let json = r#"{
      "code": 0,
      "data": {
        "dash": {
          "duration": 1,
          "video": [{
            "id": 80,
            "baseUrl": "https://video.example/camel.m4s",
            "base_url": "https://video.example/snake.m4s",
            "backupUrl": ["https://video.example/camel-backup.m4s"],
            "backup_url": ["https://video.example/snake-backup.m4s"],
            "bandwidth": 1,
            "codecid": 7,
            "codecs": "avc1",
            "width": 1920,
            "height": 1080,
            "frameRate": "30.000",
            "frame_rate": "30.000"
          }],
          "audio": [{
            "id": 30280,
            "baseUrl": "https://audio.example/camel.m4s",
            "base_url": "https://audio.example/snake.m4s",
            "backupUrl": [],
            "backup_url": [],
            "bandwidth": 1,
            "codecs": "mp4a.40.2"
          }]
        }
      }
    }"#;

    let parsed = parse_play_response(json).unwrap();

    assert_eq!(
        parsed.video_tracks[0].base_url,
        "https://video.example/snake.m4s"
    );
    assert_eq!(
        parsed.audio_tracks[0].base_url,
        "https://audio.example/snake.m4s"
    );
}
