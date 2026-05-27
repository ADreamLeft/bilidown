use anyhow::Context;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoInput {
    Aid(u64),
    Bvid(String),
}

pub fn parse_video_input(input: &str) -> anyhow::Result<VideoInput> {
    let input = input.trim();
    if input.is_empty() {
        anyhow::bail!("empty video input");
    }

    let bv_re = Regex::new(r"(?i)\bBV[0-9A-Za-z]{10}\b").unwrap();
    if let Some(mat) = bv_re.find(input) {
        return Ok(VideoInput::Bvid(normalize_bvid(mat.as_str())));
    }

    let av_re = Regex::new(r"(?i)(?:^|/|video/)av(\d+)").unwrap();
    if let Some(caps) = av_re.captures(input) {
        let aid = caps
            .get(1)
            .context("missing av id")?
            .as_str()
            .parse::<u64>()
            .context("invalid av id")?;
        return Ok(VideoInput::Aid(aid));
    }

    if input.chars().all(|c| c.is_ascii_digit()) {
        return Ok(VideoInput::Aid(
            input.parse::<u64>().context("invalid aid")?,
        ));
    }

    anyhow::bail!("unsupported input; expected BV, av, or a normal bilibili video URL")
}

fn normalize_bvid(s: &str) -> String {
    let mut chars = s.chars();
    let b = chars.next().unwrap_or('B').to_ascii_uppercase();
    let v = chars.next().unwrap_or('V').to_ascii_uppercase();
    format!("{b}{v}{}", chars.collect::<String>())
}
