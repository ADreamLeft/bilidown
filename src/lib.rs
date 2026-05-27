pub mod auth;
pub mod client;
pub mod config;
pub mod download;
pub mod fs_utils;
pub mod input;
pub mod mux;
pub mod page;
pub mod video;
pub mod wbi;

pub const USER_AGENT: &str = concat!(
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) ",
    "AppleWebKit/537.36 (KHTML, like Gecko) ",
    "Chrome/133.0.0.0 Safari/537.36"
);

pub const REFERER: &str = "https://www.bilibili.com/";
