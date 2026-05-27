use std::time::Duration;

use anyhow::Context;
use qrcode::{QrCode, render::unicode};
use serde::Deserialize;

use crate::{
    client::BiliClient,
    config::{self, AuthState},
};

const QR_GENERATE_URL: &str = "https://passport.bilibili.com/x/passport-login/web/qrcode/generate";
const QR_POLL_URL: &str = "https://passport.bilibili.com/x/passport-login/web/qrcode/poll";

pub async fn login(client: &BiliClient) -> anyhow::Result<()> {
    let generated: QrGenerateResponse = client.get_json(QR_GENERATE_URL).await?;
    if generated.code != 0 {
        anyhow::bail!(
            "QR generate failed: code={}, message={}",
            generated.code,
            generated.message.unwrap_or_default()
        );
    }
    let data = generated.data.context("QR generate returned no data")?;

    let code = QrCode::new(data.url.as_bytes()).context("build QR code")?;
    let qr = code
        .render::<unicode::Dense1x2>()
        .quiet_zone(true)
        .module_dimensions(2, 1)
        .build();

    println!("{qr}");
    println!("请使用 Bilibili 手机客户端扫描二维码并确认登录。");

    loop {
        tokio::time::sleep(Duration::from_secs(3)).await;
        let url = format!("{QR_POLL_URL}?qrcode_key={}", data.qrcode_key);
        let polled: QrPollResponse = client.get_json(&url).await?;
        if polled.code != 0 {
            anyhow::bail!(
                "QR poll failed: code={}, message={}",
                polled.code,
                polled.message.unwrap_or_default()
            );
        }
        let poll = polled.data.context("QR poll returned no data")?;
        match poll.code {
            0 => {
                client.save_cookies()?;
                let refresh_token = poll
                    .refresh_token
                    .or_else(|| extract_refresh_token(poll.url.as_deref().unwrap_or_default()));
                config::write_auth_state(&AuthState { refresh_token })?;
                println!("登录成功，cookie 已保存。");
                return Ok(());
            }
            86101 => println!("等待扫码..."),
            86090 => println!("已扫码，等待确认..."),
            86038 => anyhow::bail!("二维码已过期，请重新运行 `bilidown login`"),
            code => anyhow::bail!(
                "unexpected QR poll status: code={}, message={}",
                code,
                poll.message.unwrap_or_default()
            ),
        }
    }
}

pub async fn status(client: &BiliClient) -> anyhow::Result<()> {
    let status = client.status().await?;
    if status.is_login {
        println!(
            "已登录：{} ({})",
            status.uname.unwrap_or_else(|| "<unknown>".to_string()),
            status
                .mid
                .map(|mid| mid.to_string())
                .unwrap_or_else(|| "unknown mid".to_string())
        );
    } else {
        println!("未登录。需要访问高画质或会员权限内容时，请先运行 `bilidown login`。");
    }
    Ok(())
}

fn extract_refresh_token(raw_url: &str) -> Option<String> {
    let url = url::Url::parse(raw_url).ok()?;
    url.query_pairs()
        .find(|(key, _)| key == "refresh_token")
        .map(|(_, value)| value.into_owned())
}

#[derive(Debug, Deserialize)]
struct QrGenerateResponse {
    code: i64,
    message: Option<String>,
    data: Option<QrGenerateData>,
}

#[derive(Debug, Deserialize)]
struct QrGenerateData {
    url: String,
    qrcode_key: String,
}

#[derive(Debug, Deserialize)]
struct QrPollResponse {
    code: i64,
    message: Option<String>,
    data: Option<QrPollData>,
}

#[derive(Debug, Deserialize)]
struct QrPollData {
    code: i64,
    message: Option<String>,
    url: Option<String>,
    refresh_token: Option<String>,
}
