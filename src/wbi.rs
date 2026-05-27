use std::collections::BTreeMap;

use md5::{Digest, Md5};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

const MIXIN_KEY_ENC_TAB: [usize; 64] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49, 33, 9, 42, 19, 29,
    28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0, 1, 60, 51, 30, 4, 22, 25,
    54, 21, 56, 59, 6, 63, 57, 62, 11, 36, 20, 34, 44, 52,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedParams {
    pairs: Vec<(String, String)>,
    w_rid: String,
}

impl SignedParams {
    pub fn to_query_string(&self) -> String {
        let mut query = encode_pairs(&self.pairs);
        query.push_str("&w_rid=");
        query.push_str(&self.w_rid);
        query
    }
}

pub fn mixin_key(img_key: &str, sub_key: &str) -> String {
    let orig = format!("{img_key}{sub_key}");
    MIXIN_KEY_ENC_TAB
        .iter()
        .filter_map(|&i| orig.as_bytes().get(i).copied())
        .take(32)
        .map(char::from)
        .collect()
}

pub fn sign_params_at(
    mut params: BTreeMap<String, String>,
    mixin_key: &str,
    wts: i64,
) -> SignedParams {
    params.insert("wts".to_string(), wts.to_string());

    let pairs = params
        .into_iter()
        .map(|(k, v)| (filter_wbi_value(&k), filter_wbi_value(&v)))
        .collect::<Vec<_>>();

    let query = encode_pairs(&pairs);
    let mut hasher = Md5::new();
    hasher.update(query.as_bytes());
    hasher.update(mixin_key.as_bytes());
    let w_rid = format!("{:x}", hasher.finalize());

    SignedParams { pairs, w_rid }
}

pub fn sign_params(params: BTreeMap<String, String>, mixin_key: &str) -> SignedParams {
    let wts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default();
    sign_params_at(params, mixin_key, wts)
}

fn filter_wbi_value(input: &str) -> String {
    input
        .chars()
        .filter(|c| !matches!(c, '!' | '\'' | '(' | ')' | '*'))
        .collect()
}

fn encode_pairs(pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                utf8_percent_encode(key, NON_ALPHANUMERIC),
                utf8_percent_encode(value, NON_ALPHANUMERIC)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}
