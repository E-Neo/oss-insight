use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use anyhow::Result;
use oss_insight_source::{SearchOrder, SearchSort};

use crate::commands::config::Config;
use crate::commands::source::github_from_config;
use crate::commands::workflow::{now, resolve_created, search_query};

pub async fn run(config: &Config, output: &Path) -> Result<()> {
    let mut github = github_from_config(config);
    let trending = &config.source.github.trending;

    let file = File::create(output)?;
    let mut writer = BufWriter::new(file);

    for lang in &trending.languages {
        for period in &trending.periods {
            let repos = github.trending(lang, period.as_str()).await?.data;
            for repo in repos {
                writeln!(writer, "{}", repo.id)?;
            }
            writer.flush()?;
        }
    }

    let search = &trending.search;
    for lang in &trending.languages {
        let created = resolve_created(&search.created, now());
        let query = search_query(lang, &search.stars, &created);
        for page in 1..=search.max_pages {
            let resp = github
                .search_repos(&query, page, SearchSort::Updated, SearchOrder::Desc)
                .await?;
            if resp.data.items.is_empty() {
                break;
            }
            for repo in resp.data.items {
                writeln!(writer, "{}", repo.id)?;
            }
            writer.flush()?;
        }
    }
    Ok(())
}
