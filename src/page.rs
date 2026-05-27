use std::collections::BTreeSet;

pub fn select_pages(expr: &str, total: usize) -> anyhow::Result<Vec<usize>> {
    if total == 0 {
        anyhow::bail!("video has no pages");
    }

    let expr = expr.trim();
    if expr.eq_ignore_ascii_case("all") {
        return Ok((1..=total).collect());
    }

    let mut selected = BTreeSet::new();
    for raw_part in expr.split(',') {
        let part = raw_part.trim();
        if part.is_empty() {
            continue;
        }

        let part = match part.to_ascii_lowercase().as_str() {
            "last" | "latest" | "new" => total.to_string(),
            _ => part.to_string(),
        };

        if let Some((start, end)) = part.split_once('-') {
            let start = parse_page_num(start, total)?;
            let end = parse_page_num(end, total)?;
            if start > end {
                anyhow::bail!("invalid page range {start}-{end}");
            }
            for page in start..=end {
                selected.insert(page);
            }
        } else {
            selected.insert(parse_page_num(&part, total)?);
        }
    }

    if selected.is_empty() {
        anyhow::bail!("empty page selection");
    }

    Ok(selected.into_iter().collect())
}

fn parse_page_num(raw: &str, total: usize) -> anyhow::Result<usize> {
    let page = raw
        .trim()
        .parse::<usize>()
        .map_err(|_| anyhow::anyhow!("invalid page number: {raw}"))?;
    if !(1..=total).contains(&page) {
        anyhow::bail!("page {page} out of range 1..={total}");
    }
    Ok(page)
}
