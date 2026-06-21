use bilidown::search::{
    SearchResult, SearchType, parse_search_response, resolve_duration, resolve_order,
};

#[test]
fn parses_video_search_results() {
    let json = include_str!("fixtures/search_video.json");
    let results = parse_search_response(json, SearchType::Video).unwrap();
    assert_eq!(results.len(), 2);
    match &results[0] {
        SearchResult::Video(v) => {
            assert_eq!(v.bvid, "BV1xx411c7XD");
            assert_eq!(v.aid, 11111);
            // <em> 高亮标签被去掉，&amp; 实体被解码
            assert_eq!(v.title, "测试视频 标题 & 符号");
            assert_eq!(v.author, "测试UP");
            assert_eq!(v.play, 12345);
            assert_eq!(v.danmaku, 67);
            assert_eq!(v.duration, "12:34");
            assert_eq!(v.typename, "科技");
        }
        other => panic!("expected video, got {other:?}"),
    }
}

#[test]
fn parses_user_search_results() {
    let json = include_str!("fixtures/search_user.json");
    let results = parse_search_response(json, SearchType::User).unwrap();
    assert_eq!(results.len(), 1);
    match &results[0] {
        SearchResult::User(u) => {
            assert_eq!(u.mid, 123456);
            assert_eq!(u.uname, "某个UP主");
            assert_eq!(u.fans, 98765);
            assert_eq!(u.videos, 42);
            assert_eq!(u.level, 6);
            assert_eq!(u.sign, "这是签名");
        }
        other => panic!("expected user, got {other:?}"),
    }
}

#[test]
fn resolve_order_maps_per_type() {
    assert_eq!(
        resolve_order(SearchType::Video, "default").unwrap(),
        "totalrank"
    );
    assert_eq!(resolve_order(SearchType::Video, "play").unwrap(), "click");
    assert_eq!(resolve_order(SearchType::Video, "danmaku").unwrap(), "dm");
    assert_eq!(resolve_order(SearchType::User, "fans").unwrap(), "fans");
    assert_eq!(resolve_order(SearchType::User, "default").unwrap(), "0");
    // 跨类型的排序值应当报错
    assert!(resolve_order(SearchType::Video, "fans").is_err());
    assert!(resolve_order(SearchType::User, "play").is_err());
}

#[test]
fn resolve_duration_maps_values() {
    assert_eq!(resolve_duration("all").unwrap(), 0);
    assert_eq!(resolve_duration("short").unwrap(), 1);
    assert_eq!(resolve_duration("verylong").unwrap(), 4);
    assert!(resolve_duration("nonsense").is_err());
}

#[test]
fn search_error_code_is_reported() {
    let json = r#"{"code":-412,"message":"请求被拦截","data":null}"#;
    let err = parse_search_response(json, SearchType::Video).unwrap_err();
    assert!(err.to_string().contains("-412"));
}
