# 测试、验证与完成度

## 自动化测试

项目带 **31 个测试**，分布在多个测试目标里：

| 测试文件 | 数量 | 覆盖内容 |
|---|---|---|
| `tests/core.rs` | 9 | DASH 流解析、最优流选择、分 P 表达式、文件名清洗、WBI 签名向量 |
| `tests/assets_config_batch.rs` | 7 | 配置读写、批量输入解析、附加资源 |
| `tests/search.rs` | 5 | 搜索结果解析、排序映射、时长过滤、错误码、HTML 去标签 |
| `tests/bangumi.rs` | 4 | 番剧输入识别、剧集列表解析、错误码 |
| `tests/download.rs` | 3 | 断点续传、备用 URL fallback、并发分片下载 |
| `tests/cli.rs` | 3 | 命令行参数解析与冲突校验 |

解析类测试用**固定的 JSON fixture**（`tests/fixtures/` 下的 `playurl_dash.json`、`search_video.json`、`search_user.json`、`season_bangumi.json`），离线即可运行，不依赖网络。例如番剧输入识别的测试，既验证能正确识别 `ep`/`ss` 及播放页 URL，也验证**不会把普通 BV 号误判成番剧**：

```rust
#[test]
fn does_not_treat_normal_videos_as_bangumi() {
    assert_eq!(parse_bangumi_input("BV1ss411c7XD"), None);          // BV 里含 "ss" 也不误判
    assert_eq!(parse_bangumi_input("av170001"), None);
    assert_eq!(parse_bangumi_input("https://example.com/recipes/ss123-soup"), None);
}
```

下载相关测试（续传、备用 URL、分片）会在本地起一个临时 HTTP 服务来模拟 CDN 行为，验证 Range 请求和续传逻辑的正确性。

## 持续集成（CI）

仓库配置了 GitHub Actions，每次 push / PR 都跑同一套检查，与本地一致：

```bash
cargo fmt --check                          # 代码格式
cargo clippy --all-targets -- -D warnings  # 静态检查，警告即失败
cargo test                                 # 全部测试
```

`clippy -D warnings` 把所有 lint 警告当作错误，强制代码保持整洁。这套"格式 + 静态检查 + 测试"三件套是项目质量的底线。

## 真实接口验证

除了离线测试，关键功能都在真实的 B 站接口上验证过：

- **搜索**：按综合 / 播放量 / 粉丝等不同排序，video 与 user 两类都能返回正确结果；
- **番剧**：`info ss<id>` 正确列出整季剧集；`download ep<id>` 正确下载到对应那一集（而非误下第一集），下载产物经 `ffprobe` 确认是合法的 mp4（AV1 视频 + AAC 音频、时长正确）；
- **回归**：加入番剧支持时重构了下载主流程，重构后普通视频下载行为与重构前完全一致。