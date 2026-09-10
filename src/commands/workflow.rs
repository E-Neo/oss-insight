use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use oss_insight_db::{Db, Workflow, WorkflowItem};
use oss_insight_source::{Github, SearchOrder, SearchSort, TrendingRepo};

use crate::commands::config::Config;
use crate::commands::source::github_from_config;

const WORKFLOW: &str = "github-trending";

pub async fn run(config: &Config) -> Result<()> {
    let db = Db::open(&config.db_path()?).await?;
    let mut github = github_from_config(config);
    let now = now();
    let tasks = build_tasks(config);
    let ttl = config.workflow.ttl_secs as i64;

    let (crawl_id, run_at) = match decision(db.last_workflow(WORKFLOW).await?, now, ttl) {
        Decision::Finished => {
            tracing::info!("workflow already finished");
            return Ok(());
        }
        Decision::Resume { last } => {
            tracing::info!(workflow_id = last.id, "resuming workflow");
            db.ensure_workflow_items(last.id, &tasks).await?;
            (last.id, last.created_at)
        }
        Decision::New => {
            let crawl = db.create_workflow(WORKFLOW, now, &tasks).await?;
            tracing::info!(workflow_id = crawl.id, "starting new workflow");
            (crawl.id, crawl.created_at)
        }
    };

    for item in db.pending_workflow_items(crawl_id).await? {
        tracing::info!(task = %item.task, "processing task");
        match run_task(&mut github, &db, config, &item, run_at).await {
            Ok(()) => db.mark_workflow_item_done(crawl_id, &item.task).await?,
            Err(e) => tracing::warn!(
                task = %item.task,
                error = ?e,
                "task failed; will retry on the next run"
            ),
        }
    }

    if db.pending_workflow_items(crawl_id).await?.is_empty() {
        db.mark_workflow_done(crawl_id).await?;
        tracing::info!(workflow_id = crawl_id, "workflow finished");
    }
    Ok(())
}

enum Decision {
    New,
    Resume { last: Workflow },
    Finished,
}

fn decision(last: Option<Workflow>, now: i64, ttl: i64) -> Decision {
    match last {
        Some(last) if now - last.created_at <= ttl => {
            if last.status == "done" {
                Decision::Finished
            } else {
                Decision::Resume { last }
            }
        }
        _ => Decision::New,
    }
}

fn build_tasks(config: &Config) -> Vec<WorkflowItem> {
    let trending = &config.source.github.trending;
    let mut tasks = Vec::new();
    for lang in &trending.languages {
        for period in &trending.periods {
            tasks.push(WorkflowItem {
                task: format!("trending:{lang}:{}", period.as_str()),
                params: serde_json::json!({
                    "source": "github/trending",
                    "language": lang,
                    "period": period.as_str(),
                })
                .to_string(),
                status: "pending".to_string(),
            });
        }
    }
    for lang in &trending.languages {
        tasks.push(WorkflowItem {
            task: format!("search:{lang}"),
            params: serde_json::json!({
                "source": "github/search",
                "language": lang,
                "stars": trending.search.stars,
                "created": trending.search.created,
                "sort": "updated",
                "order": "desc",
                "max_pages": trending.search.max_pages,
            })
            .to_string(),
            status: "pending".to_string(),
        });
    }
    tasks
}

async fn run_task(
    github: &mut Github,
    db: &Db,
    config: &Config,
    item: &WorkflowItem,
    run_at: i64,
) -> Result<()> {
    let params: serde_json::Value = serde_json::from_str(&item.params)?;
    match params["source"].as_str() {
        Some("github/trending") => {
            let lang = params["language"].as_str().unwrap_or_default();
            let period = params["period"].as_str().context("trending period")?;
            let repos = github.trending(lang, period).await?.data;
            for repo in repos {
                sync_trending_repo(github, db, config, &repo, &params, run_at).await?;
            }
        }
        Some("github/search") => {
            let lang = params["language"].as_str().unwrap_or_default();
            let stars = params["stars"].as_str().context("search stars")?;
            let created = params["created"].as_str().context("search created")?;
            let max_pages = params["max_pages"].as_u64().context("search max_pages")? as u32;
            let query = search_query(lang, stars, created);
            for page in 1..=max_pages {
                let search = github
                    .search_repos(&query, page, SearchSort::Updated, SearchOrder::Desc)
                    .await?;
                if search.data.items.is_empty() {
                    break;
                }
                for repo in search.data.items {
                    db.upsert_repo(&repo).await?;
                    db.insert_trending(
                        "github/search",
                        Some(repo.id),
                        Some(&repo.full_name),
                        Some(&params),
                        run_at,
                    )
                    .await?;
                }
            }
        }
        other => anyhow::bail!("unknown task source: {other:?}"),
    }
    Ok(())
}

async fn sync_trending_repo(
    github: &mut Github,
    db: &Db,
    config: &Config,
    repo: &TrendingRepo,
    params: &serde_json::Value,
    run_at: i64,
) -> Result<()> {
    let now = now();
    let ttl = config.db.ttl_secs as i64;

    let mut owner_id = None;
    let mut owner_login = None;
    if is_stale(db.repo_updated_at(repo.id).await?, now, ttl) {
        let resp = github.repo(&repo.full_name).await?;
        db.upsert_repo(&resp.data).await?;
        owner_id = Some(resp.data.owner.id);
        owner_login = Some(resp.data.owner.login.clone());
    } else if let Some((id, login)) = db.repo_owner(repo.id).await? {
        owner_id = Some(id as u64);
        owner_login = Some(login);
    }
    if let (Some(id), Some(login)) = (owner_id, owner_login)
        && is_stale(db.user_full_updated_at(id).await?, now, ttl)
    {
        let user = github.user(&login).await?;
        db.upsert_user(&user.data).await?;
    }

    let history_updated = db.star_history_updated_at(repo.id).await?;
    if history_updated.is_none() || is_stale(history_updated, now, ttl) {
        let incremental = history_updated.is_some();
        let mut weeks = Vec::new();
        for page in 1.. {
            let resp = github.stargazer_history(&repo.full_name, page).await?;
            if resp.data.is_empty() {
                break;
            }
            weeks.extend(resp.data);
            if incremental {
                break;
            }
        }
        db.insert_star_history(repo.id, &weeks, run_at).await?;
    }

    if is_stale(db.readme_updated_at(repo.id).await?, now, ttl) {
        let readme = github.readme(&repo.full_name).await?;
        db.upsert_readme(repo.id, &readme.data).await?;
    }

    db.insert_trending(
        "github/trending",
        Some(repo.id),
        Some(&repo.full_name),
        Some(params),
        run_at,
    )
    .await?;
    Ok(())
}

fn search_query(lang: &str, stars: &str, created: &str) -> String {
    let mut query = String::new();
    if !lang.is_empty() {
        query.push_str("language:");
        query.push_str(lang);
        query.push(' ');
    }
    query.push_str("stars:");
    query.push_str(stars);
    query.push(' ');
    query.push_str("created:");
    query.push_str(created);
    query
}

fn is_stale(updated_at: Option<i64>, now: i64, ttl: i64) -> bool {
    updated_at.is_none_or(|u| now - u > ttl)
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workflow(id: i64, status: &str, created_at: i64) -> Workflow {
        Workflow {
            id,
            workflow: WORKFLOW.to_string(),
            status: status.to_string(),
            created_at,
            updated_at: created_at,
        }
    }

    #[test]
    fn decision_within_ttl_done_exits() {
        let last = Some(workflow(1, "done", 1000));
        assert!(matches!(decision(last, 1100, 3600), Decision::Finished));
    }

    #[test]
    fn decision_within_ttl_running_resumes() {
        let last = Some(workflow(1, "running", 1000));
        assert!(matches!(
            decision(last, 1100, 3600),
            Decision::Resume { last: _ }
        ));
    }

    #[test]
    fn decision_past_ttl_starts_new() {
        let last = Some(workflow(1, "running", 1000));
        assert!(matches!(decision(last, 5000, 3600), Decision::New));
        assert!(matches!(decision(None, 5000, 3600), Decision::New));
    }
}
