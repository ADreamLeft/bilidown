use std::collections::BTreeMap;

use bilidown::{
    fs_utils::safe_path_component,
    input::{VideoInput, parse_video_input},
    page::select_pages,
    video::{AudioQualityPreference, CodecPreference, QualityPreference, parse_play_response},
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
