use bilidown::bangumi::{BangumiInput, parse_bangumi_input, parse_season_response};

#[test]
fn parses_bangumi_inputs() {
    assert_eq!(
        parse_bangumi_input("ep374660"),
        Some(BangumiInput::Ep(374660))
    );
    assert_eq!(
        parse_bangumi_input("ss33802"),
        Some(BangumiInput::Season(33802))
    );
    assert_eq!(
        parse_bangumi_input("https://www.bilibili.com/bangumi/play/ep374660"),
        Some(BangumiInput::Ep(374660))
    );
    assert_eq!(
        parse_bangumi_input("https://www.bilibili.com/bangumi/play/ss33802"),
        Some(BangumiInput::Season(33802))
    );
    // query 形式（?ep_id= / ?season_id=）
    assert_eq!(
        parse_bangumi_input("https://www.bilibili.com/bangumi/play/ss33802?ep_id=374660"),
        Some(BangumiInput::Ep(374660))
    );
    assert_eq!(
        parse_bangumi_input("season_id=33802"),
        Some(BangumiInput::Season(33802))
    );
}

#[test]
fn does_not_treat_normal_videos_as_bangumi() {
    assert_eq!(parse_bangumi_input("BV1ss411c7XD"), None);
    assert_eq!(
        parse_bangumi_input("https://www.bilibili.com/video/BV1xx411c7XD"),
        None
    );
    assert_eq!(parse_bangumi_input("av170001"), None);
    assert_eq!(parse_bangumi_input("170001"), None);
    // 非 B站番剧路径里的 ss/ep 不应误判
    assert_eq!(
        parse_bangumi_input("https://example.com/recipes/ss123-soup"),
        None
    );
    assert_eq!(parse_bangumi_input("https://shop.com/ep42-special"), None);
}

#[test]
fn parses_season_response() {
    let json = include_str!("fixtures/season_bangumi.json");
    let season = parse_season_response(json).unwrap();
    // season_title 优先于 title
    assert_eq!(season.title, "测试番剧");
    assert_eq!(season.episodes.len(), 2);

    let ep1 = &season.episodes[0];
    assert_eq!(ep1.index, 1);
    assert_eq!(ep1.ep_id, 1001);
    assert_eq!(ep1.aid, 11111);
    assert_eq!(ep1.cid, 22222);
    assert_eq!(ep1.bvid, "BV1aa4y1x7AA");
    assert_eq!(ep1.title, "1");
    assert_eq!(ep1.long_title, "第一集");

    let ep2 = &season.episodes[1];
    assert_eq!(ep2.index, 2);
    assert_eq!(ep2.cid, 44444);
}

#[test]
fn season_error_code_is_reported() {
    let json = r#"{"code":-404,"message":"啥都木有","result":null}"#;
    let err = parse_season_response(json).unwrap_err();
    assert!(err.to_string().contains("-404"));
}
