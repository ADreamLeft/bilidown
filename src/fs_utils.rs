use std::{collections::BTreeMap, path::PathBuf};

use anyhow::Context;

pub fn project_dirs() -> anyhow::Result<directories::ProjectDirs> {
    directories::ProjectDirs::from("org", "adl", "bilidown")
        .context("could not resolve bilidown project directories")
}

pub fn safe_path_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        if c.is_control() || matches!(c, '"' | '<' | '>' | '|' | ':' | '*' | '?' | '\\' | '/') {
            out.push('_');
        } else {
            out.push(c);
        }
    }

    let out = out.trim_matches([' ', '.']).to_string();
    if out.is_empty() { "_".to_string() } else { out }
}

pub fn render_output_path(
    out_dir: impl Into<PathBuf>,
    template: &str,
    vars: &BTreeMap<&str, String>,
) -> PathBuf {
    let mut rendered = template.to_string();
    for (key, value) in vars {
        rendered = rendered.replace(&format!("{{{key}}}"), value);
    }

    let mut path = out_dir.into();
    for part in rendered.split('/') {
        if !part.is_empty() {
            path.push(safe_path_component(part));
        }
    }
    path
}
