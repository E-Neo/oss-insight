use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use oss_insight_source::{Readme, Repo, SimpleUser, StargazerHistory, User};
use sqlx::FromRow;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Base64(#[from] base64::DecodeError),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
}

pub type Result<T> = std::result::Result<T, DbError>;

const MIGRATIONS: [&str; 7] = [
    "CREATE TABLE IF NOT EXISTS github_users (
        id                INTEGER PRIMARY KEY,
        login             TEXT    NOT NULL UNIQUE,
        node_id           TEXT,
        avatar_url        TEXT,
        gravatar_id       TEXT,
        html_url          TEXT,
        type              TEXT    NOT NULL,
        user_view_type    TEXT,
        site_admin        INTEGER DEFAULT 0 NOT NULL,
        name              TEXT,
        company           TEXT,
        blog              TEXT,
        location          TEXT,
        email             TEXT,
        hireable          INTEGER,
        bio               TEXT,
        twitter_username  TEXT,
        public_repos      INTEGER,
        public_gists      INTEGER,
        followers         INTEGER,
        following         INTEGER,
        github_created_at TEXT,
        github_updated_at TEXT,
        full_updated_at   INTEGER,
        created_at        INTEGER NOT NULL,
        updated_at        INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS github_repos (
        id                           INTEGER PRIMARY KEY,
        full_name                    TEXT    NOT NULL UNIQUE,
        node_id                      TEXT,
        name                         TEXT    NOT NULL,
        owner_id                     INTEGER REFERENCES github_users(id),
        organization_id              INTEGER REFERENCES github_users(id),
        private                      INTEGER DEFAULT 0 NOT NULL,
        html_url                     TEXT,
        description                  TEXT,
        fork                         INTEGER DEFAULT 0 NOT NULL,
        github_created_at            TEXT,
        github_updated_at            TEXT,
        pushed_at                    TEXT,
        size                         INTEGER,
        stargazers_count             INTEGER,
        watchers_count               INTEGER,
        language                     TEXT,
        forks_count                  INTEGER,
        open_issues_count            INTEGER,
        mirror_url                   TEXT,
        archived                     INTEGER DEFAULT 0 NOT NULL,
        disabled                     INTEGER DEFAULT 0 NOT NULL,
        license_key                  TEXT,
        license_name                 TEXT,
        license_spdx_id              TEXT,
        topics                       TEXT,
        visibility                   TEXT,
        default_branch               TEXT,
        allow_forking                INTEGER,
        is_template                  INTEGER,
        web_commit_signoff_required  INTEGER,
        has_issues                   INTEGER,
        has_projects                 INTEGER,
        has_downloads                INTEGER,
        has_wiki                     INTEGER,
        has_pages                    INTEGER,
        has_discussions              INTEGER,
        has_pull_requests            INTEGER,
        pull_request_creation_policy TEXT,
        network_count                INTEGER,
        subscribers_count            INTEGER,
        created_at                   INTEGER NOT NULL,
        updated_at                   INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS github_trendings (
        id         INTEGER PRIMARY KEY,
        source     TEXT    NOT NULL,
        repo_id    INTEGER REFERENCES github_repos(id),
        full_name  TEXT,
        params     TEXT,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        CHECK (repo_id IS NOT NULL OR full_name IS NOT NULL),
        UNIQUE (source, repo_id, full_name, params, created_at)
    )",
    "CREATE TABLE IF NOT EXISTS github_star_history (
        repo_id    INTEGER NOT NULL REFERENCES github_repos(id),
        week       INTEGER NOT NULL,
        total      INTEGER NOT NULL,
        days       TEXT    NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        PRIMARY KEY (repo_id, week)
    )",
    "CREATE TABLE IF NOT EXISTS github_readmes (
        repo_id    INTEGER NOT NULL REFERENCES github_repos(id),
        name       TEXT    NOT NULL,
        content    TEXT,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        PRIMARY KEY (repo_id)
    )",
    "CREATE TABLE IF NOT EXISTS github_workflows (
        id          INTEGER PRIMARY KEY,
        workflow    TEXT    NOT NULL,
        status      TEXT    NOT NULL,
        created_at  INTEGER NOT NULL,
        updated_at  INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS github_workflow_items (
        workflow_id INTEGER NOT NULL REFERENCES github_workflows(id),
        task        TEXT    NOT NULL,
        params      TEXT    NOT NULL,
        status      TEXT    NOT NULL DEFAULT 'pending',
        created_at  INTEGER NOT NULL,
        updated_at  INTEGER NOT NULL,
        PRIMARY KEY (workflow_id, task)
    )",
];

pub struct Db {
    pool: SqlitePool,
}

impl Db {
    pub async fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal);
        Self::from_connect_options(options).await
    }

    async fn from_connect_options(options: SqliteConnectOptions) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        let db = Db { pool };
        db.migrate().await?;
        Ok(db)
    }

    async fn migrate(&self) -> Result<()> {
        for stmt in MIGRATIONS {
            sqlx::query(stmt).execute(&self.pool).await?;
        }
        Ok(())
    }

    pub async fn upsert_user(&self, user: &User) -> Result<()> {
        self.insert_user_row(&UserRow::from(user), Some(now()))
            .await
    }

    pub async fn upsert_repo(&self, repo: &Repo) -> Result<()> {
        let owner = UserRow::from(&repo.owner);
        let organization = repo.organization.as_ref().map(UserRow::from);
        self.insert_user_row(&owner, None).await?;
        if let Some(org) = &organization {
            self.insert_user_row(org, None).await?;
        }
        let repo_row = RepoRow::from(repo);
        let owner_id = owner.id;
        let organization_id = organization.as_ref().map(|o| o.id);
        self.insert_repo_row(&repo_row, owner_id, organization_id)
            .await
    }

    pub async fn ensure_repo(&self, id: u64, full_name: &str) -> Result<()> {
        let now = now();
        let name = full_name
            .rsplit_once('/')
            .map(|(_, name)| name.to_string())
            .unwrap_or_else(|| full_name.to_string());
        sqlx::query(
            "INSERT INTO github_repos (id, full_name, name, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(id as i64)
        .bind(full_name)
        .bind(name)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_trending(
        &self,
        source: &str,
        repo_id: Option<u64>,
        full_name: Option<&str>,
        params: Option<&serde_json::Value>,
        created_at: i64,
    ) -> Result<()> {
        let params = params.map(serde_json::Value::to_string);
        sqlx::query(
            "INSERT INTO github_trendings (source, repo_id, full_name, params, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT DO NOTHING",
        )
        .bind(source)
        .bind(repo_id.map(|id| id as i64))
        .bind(full_name)
        .bind(params.as_deref())
        .bind(created_at)
        .bind(created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert_star_history(
        &self,
        repo_id: u64,
        weeks: &[StargazerHistory],
        created_at: i64,
    ) -> Result<()> {
        for week in weeks {
            let days = serde_json::to_string(&week.days)?;
            sqlx::query(
                "INSERT INTO github_star_history (repo_id, week, total, days, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?)
                 ON CONFLICT(repo_id, week) DO UPDATE
                 SET total = excluded.total,
                     days = excluded.days,
                     updated_at = excluded.updated_at",
            )
            .bind(repo_id as i64)
            .bind(week.week as i64)
            .bind(week.total as i64)
            .bind(days)
            .bind(created_at)
            .bind(created_at)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn upsert_readme(&self, repo_id: u64, readme: &Readme) -> Result<()> {
        let content = if readme.encoding == "base64" {
            let bytes = BASE64.decode(&readme.content)?;
            String::from_utf8(bytes)?
        } else {
            readme.content.clone()
        };
        let now = now();
        sqlx::query(
            "INSERT INTO github_readmes (repo_id, name, content, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(repo_id) DO UPDATE
             SET name = excluded.name,
                 content = excluded.content,
                 updated_at = excluded.updated_at",
        )
        .bind(repo_id as i64)
        .bind(&readme.name)
        .bind(content)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn last_workflow(&self, workflow: &str) -> Result<Option<Workflow>> {
        Ok(sqlx::query_as::<_, Workflow>(
            "SELECT id, workflow, status, created_at, updated_at
               FROM github_workflows
              WHERE workflow = ?
              ORDER BY id DESC
              LIMIT 1",
        )
        .bind(workflow)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn create_workflow(
        &self,
        workflow: &str,
        created_at: i64,
        items: &[WorkflowItem],
    ) -> Result<Workflow> {
        let result = sqlx::query(
            "INSERT INTO github_workflows (workflow, status, created_at, updated_at)
             VALUES (?, 'running', ?, ?)",
        )
        .bind(workflow)
        .bind(created_at)
        .bind(created_at)
        .execute(&self.pool)
        .await?;
        let id = result.last_insert_rowid();
        self.ensure_workflow_items(id, items).await?;
        Ok(Workflow {
            id,
            workflow: workflow.to_string(),
            status: "running".to_string(),
            created_at,
            updated_at: created_at,
        })
    }

    pub async fn ensure_workflow_items(
        &self,
        workflow_id: i64,
        items: &[WorkflowItem],
    ) -> Result<()> {
        let now = now();
        for item in items {
            sqlx::query(
                "INSERT INTO github_workflow_items (workflow_id, task, params, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'pending', ?, ?)
                 ON CONFLICT(workflow_id, task) DO NOTHING",
            )
            .bind(workflow_id)
            .bind(&item.task)
            .bind(&item.params)
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn pending_workflow_items(&self, workflow_id: i64) -> Result<Vec<WorkflowItem>> {
        Ok(sqlx::query_as::<_, WorkflowItem>(
            "SELECT task, params, status
               FROM github_workflow_items
              WHERE workflow_id = ?
                AND status = 'pending'",
        )
        .bind(workflow_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn mark_workflow_item_done(&self, workflow_id: i64, task: &str) -> Result<()> {
        sqlx::query(
            "UPDATE github_workflow_items
                SET status = 'done',
                    updated_at = ?
              WHERE workflow_id = ?
                AND task = ?",
        )
        .bind(now())
        .bind(workflow_id)
        .bind(task)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_workflow_done(&self, workflow_id: i64) -> Result<()> {
        sqlx::query(
            "UPDATE github_workflows
                SET status = 'done',
                    updated_at = ?
              WHERE id = ?",
        )
        .bind(now())
        .bind(workflow_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn repo_updated_at(&self, id: u64) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT updated_at
               FROM github_repos
              WHERE id = ?",
        )
        .bind(id as i64)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn user_full_updated_at(&self, id: u64) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar::<_, Option<i64>>(
            "SELECT full_updated_at
               FROM github_users
              WHERE id = ?",
        )
        .bind(id as i64)
        .fetch_optional(&self.pool)
        .await?
        .flatten())
    }

    pub async fn star_history_updated_at(&self, repo_id: u64) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MAX(updated_at)
               FROM github_star_history
              WHERE repo_id = ?",
        )
        .bind(repo_id as i64)
        .fetch_optional(&self.pool)
        .await?
        .flatten())
    }

    pub async fn readme_updated_at(&self, repo_id: u64) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT updated_at
               FROM github_readmes
              WHERE repo_id = ?",
        )
        .bind(repo_id as i64)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn repo_owner(&self, repo_id: u64) -> Result<Option<(i64, String)>> {
        Ok(sqlx::query_as::<_, (i64, String)>(
            "SELECT u.id, u.login
               FROM github_repos AS r
                    JOIN github_users AS u
                      ON r.owner_id = u.id
              WHERE r.id = ?",
        )
        .bind(repo_id as i64)
        .fetch_optional(&self.pool)
        .await?)
    }

    async fn insert_user_row(&self, row: &UserRow, full_updated_at: Option<i64>) -> Result<()> {
        let now = now();
        sqlx::query(
            "INSERT INTO github_users (
                id, login, node_id, avatar_url, gravatar_id, html_url, type, user_view_type,
                site_admin, name, company, blog, location, email, hireable, bio, twitter_username,
                public_repos, public_gists, followers, following, github_created_at, github_updated_at,
                full_updated_at, created_at, updated_at
             )
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE
             SET login = excluded.login,
                 node_id = excluded.node_id,
                 avatar_url = excluded.avatar_url,
                 gravatar_id = excluded.gravatar_id,
                 html_url = excluded.html_url,
                 type = excluded.type,
                 user_view_type = excluded.user_view_type,
                 site_admin = excluded.site_admin,
                 name = excluded.name,
                 company = excluded.company,
                 blog = excluded.blog,
                 location = excluded.location,
                 email = excluded.email,
                 hireable = excluded.hireable,
                 bio = excluded.bio,
                 twitter_username = excluded.twitter_username,
                 public_repos = excluded.public_repos,
                 public_gists = excluded.public_gists,
                 followers = excluded.followers,
                 following = excluded.following,
                 github_created_at = excluded.github_created_at,
                 github_updated_at = excluded.github_updated_at,
                 full_updated_at = COALESCE(excluded.full_updated_at, github_users.full_updated_at),
                 updated_at = excluded.updated_at",
        )
        .bind(row.id)
        .bind(&row.login)
        .bind(row.node_id.as_deref())
        .bind(&row.avatar_url)
        .bind(row.gravatar_id.as_deref())
        .bind(&row.html_url)
        .bind(&row.user_type)
        .bind(row.user_view_type.as_deref())
        .bind(row.site_admin)
        .bind(row.name.as_deref())
        .bind(row.company.as_deref())
        .bind(row.blog.as_deref())
        .bind(row.location.as_deref())
        .bind(row.email.as_deref())
        .bind(row.hireable)
        .bind(row.bio.as_deref())
        .bind(row.twitter_username.as_deref())
        .bind(row.public_repos)
        .bind(row.public_gists)
        .bind(row.followers)
        .bind(row.following)
        .bind(row.github_created_at.as_deref())
        .bind(row.github_updated_at.as_deref())
        .bind(full_updated_at)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn insert_repo_row(
        &self,
        row: &RepoRow,
        owner_id: i64,
        organization_id: Option<i64>,
    ) -> Result<()> {
        let now = now();
        sqlx::query(
            "INSERT INTO github_repos (
                id, full_name, node_id, name, owner_id, organization_id, private, html_url,
                description, fork, github_created_at, github_updated_at, pushed_at, size,
                stargazers_count, watchers_count, language, forks_count, open_issues_count, mirror_url,
                archived, disabled, license_key, license_name, license_spdx_id, topics, visibility,
                default_branch, allow_forking, is_template, web_commit_signoff_required,
                has_issues, has_projects, has_downloads, has_wiki, has_pages, has_discussions,
                has_pull_requests, pull_request_creation_policy, network_count, subscribers_count,
                created_at, updated_at
             )
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE
             SET full_name = excluded.full_name,
                 node_id = excluded.node_id,
                 name = excluded.name,
                 owner_id = excluded.owner_id,
                 organization_id = excluded.organization_id,
                 private = excluded.private,
                 html_url = excluded.html_url,
                 description = excluded.description,
                 fork = excluded.fork,
                 github_created_at = excluded.github_created_at,
                 github_updated_at = excluded.github_updated_at,
                 pushed_at = excluded.pushed_at,
                 size = excluded.size,
                 stargazers_count = excluded.stargazers_count,
                 watchers_count = excluded.watchers_count,
                 language = excluded.language,
                 forks_count = excluded.forks_count,
                 open_issues_count = excluded.open_issues_count,
                 mirror_url = excluded.mirror_url,
                 archived = excluded.archived,
                 disabled = excluded.disabled,
                 license_key = excluded.license_key,
                 license_name = excluded.license_name,
                 license_spdx_id = excluded.license_spdx_id,
                 topics = excluded.topics,
                 visibility = excluded.visibility,
                 default_branch = excluded.default_branch,
                 allow_forking = excluded.allow_forking,
                 is_template = excluded.is_template,
                 web_commit_signoff_required = excluded.web_commit_signoff_required,
                 has_issues = excluded.has_issues,
                 has_projects = excluded.has_projects,
                 has_downloads = excluded.has_downloads,
                 has_wiki = excluded.has_wiki,
                 has_pages = excluded.has_pages,
                 has_discussions = excluded.has_discussions,
                 has_pull_requests = excluded.has_pull_requests,
                 pull_request_creation_policy = excluded.pull_request_creation_policy,
                 network_count = excluded.network_count,
                 subscribers_count = excluded.subscribers_count,
                 updated_at = excluded.updated_at",
        )
        .bind(row.id)
        .bind(&row.full_name)
        .bind(row.node_id.as_deref())
        .bind(&row.name)
        .bind(owner_id)
        .bind(organization_id)
        .bind(row.private)
        .bind(&row.html_url)
        .bind(row.description.as_deref())
        .bind(row.fork)
        .bind(row.github_created_at.as_deref())
        .bind(row.github_updated_at.as_deref())
        .bind(row.pushed_at.as_deref())
        .bind(row.size)
        .bind(row.stargazers_count)
        .bind(row.watchers_count)
        .bind(row.language.as_deref())
        .bind(row.forks_count)
        .bind(row.open_issues_count)
        .bind(row.mirror_url.as_deref())
        .bind(row.archived)
        .bind(row.disabled)
        .bind(row.license_key.as_deref())
        .bind(row.license_name.as_deref())
        .bind(row.license_spdx_id.as_deref())
        .bind(row.topics.as_deref())
        .bind(&row.visibility)
        .bind(&row.default_branch)
        .bind(row.allow_forking)
        .bind(row.is_template)
        .bind(row.web_commit_signoff_required)
        .bind(row.has_issues)
        .bind(row.has_projects)
        .bind(row.has_downloads)
        .bind(row.has_wiki)
        .bind(row.has_pages)
        .bind(row.has_discussions)
        .bind(row.has_pull_requests)
        .bind(row.pull_request_creation_policy.as_deref())
        .bind(row.network_count)
        .bind(row.subscribers_count)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_secs() as i64
}

struct UserRow {
    id: i64,
    login: String,
    node_id: Option<String>,
    avatar_url: String,
    gravatar_id: Option<String>,
    html_url: String,
    user_type: String,
    user_view_type: Option<String>,
    site_admin: bool,
    name: Option<String>,
    company: Option<String>,
    blog: Option<String>,
    location: Option<String>,
    email: Option<String>,
    hireable: Option<bool>,
    bio: Option<String>,
    twitter_username: Option<String>,
    public_repos: Option<i64>,
    public_gists: Option<i64>,
    followers: Option<i64>,
    following: Option<i64>,
    github_created_at: Option<String>,
    github_updated_at: Option<String>,
}

impl From<&User> for UserRow {
    fn from(u: &User) -> Self {
        Self {
            id: u.id as i64,
            login: u.login.clone(),
            node_id: u.node_id.clone(),
            avatar_url: u.avatar_url.clone(),
            gravatar_id: u.gravatar_id.clone(),
            html_url: u.html_url.clone(),
            user_type: u.r#type.clone(),
            user_view_type: u.user_view_type.clone(),
            site_admin: u.site_admin,
            name: u.name.clone(),
            company: u.company.clone(),
            blog: u.blog.clone(),
            location: u.location.clone(),
            email: u.email.clone(),
            hireable: u.hireable,
            bio: u.bio.clone(),
            twitter_username: u.twitter_username.clone(),
            public_repos: Some(u.public_repos as i64),
            public_gists: Some(u.public_gists as i64),
            followers: Some(u.followers as i64),
            following: Some(u.following as i64),
            github_created_at: Some(u.created_at.clone()),
            github_updated_at: Some(u.updated_at.clone()),
        }
    }
}

impl From<&SimpleUser> for UserRow {
    fn from(u: &SimpleUser) -> Self {
        Self {
            id: u.id as i64,
            login: u.login.clone(),
            node_id: u.node_id.clone(),
            avatar_url: u.avatar_url.clone(),
            gravatar_id: u.gravatar_id.clone(),
            html_url: u.html_url.clone(),
            user_type: u.r#type.clone(),
            user_view_type: u.user_view_type.clone(),
            site_admin: u.site_admin,
            name: u.name.clone(),
            company: None,
            blog: None,
            location: None,
            email: u.email.clone(),
            hireable: None,
            bio: None,
            twitter_username: None,
            public_repos: None,
            public_gists: None,
            followers: None,
            following: None,
            github_created_at: None,
            github_updated_at: None,
        }
    }
}

struct RepoRow {
    id: i64,
    node_id: Option<String>,
    name: String,
    full_name: String,
    private: bool,
    html_url: String,
    description: Option<String>,
    fork: bool,
    github_created_at: Option<String>,
    github_updated_at: Option<String>,
    pushed_at: Option<String>,
    size: Option<i64>,
    stargazers_count: i64,
    watchers_count: i64,
    language: Option<String>,
    forks_count: i64,
    open_issues_count: i64,
    mirror_url: Option<String>,
    archived: bool,
    disabled: bool,
    license_key: Option<String>,
    license_name: Option<String>,
    license_spdx_id: Option<String>,
    topics: Option<String>,
    visibility: String,
    default_branch: String,
    allow_forking: bool,
    is_template: bool,
    web_commit_signoff_required: bool,
    has_issues: bool,
    has_projects: bool,
    has_downloads: bool,
    has_wiki: bool,
    has_pages: bool,
    has_discussions: bool,
    has_pull_requests: bool,
    pull_request_creation_policy: Option<String>,
    network_count: i64,
    subscribers_count: i64,
}

impl From<&Repo> for RepoRow {
    fn from(r: &Repo) -> Self {
        let license = r.license.as_ref();
        Self {
            id: r.id as i64,
            node_id: r.node_id.clone(),
            name: r.name.clone(),
            full_name: r.full_name.clone(),
            private: r.private,
            html_url: r.html_url.clone(),
            description: r.description.clone(),
            fork: r.fork,
            github_created_at: Some(r.created_at.clone()),
            github_updated_at: Some(r.updated_at.clone()),
            pushed_at: r.pushed_at.clone(),
            size: Some(r.size as i64),
            stargazers_count: r.stargazers_count as i64,
            watchers_count: r.watchers_count as i64,
            language: r.language.clone(),
            forks_count: r.forks_count as i64,
            open_issues_count: r.open_issues_count as i64,
            mirror_url: r.mirror_url.clone(),
            archived: r.archived,
            disabled: r.disabled,
            license_key: license.map(|l| l.key.clone()),
            license_name: license.map(|l| l.name.clone()),
            license_spdx_id: license.and_then(|l| l.spdx_id.clone()),
            topics: serde_json::to_string(&r.topics).ok(),
            visibility: r.visibility.clone(),
            default_branch: r.default_branch.clone(),
            allow_forking: r.allow_forking,
            is_template: r.is_template,
            web_commit_signoff_required: r.web_commit_signoff_required,
            has_issues: r.has_issues,
            has_projects: r.has_projects,
            has_downloads: r.has_downloads,
            has_wiki: r.has_wiki,
            has_pages: r.has_pages,
            has_discussions: r.has_discussions,
            has_pull_requests: r.has_pull_requests,
            pull_request_creation_policy: r.pull_request_creation_policy.clone(),
            network_count: r.network_count as i64,
            subscribers_count: r.subscribers_count as i64,
        }
    }
}

#[derive(FromRow)]
pub struct Workflow {
    pub id: i64,
    pub workflow: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(FromRow)]
pub struct WorkflowItem {
    pub task: String,
    pub params: String,
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use oss_insight_source::License;

    async fn test_db() -> Db {
        let options = SqliteConnectOptions::new()
            .in_memory(true)
            .shared_cache(true);
        Db::from_connect_options(options).await.unwrap()
    }

    fn simple_user(id: u64, login: &str) -> SimpleUser {
        SimpleUser {
            login: login.to_string(),
            id,
            node_id: None,
            avatar_url: "https://avatars.example/a".to_string(),
            gravatar_id: None,
            html_url: format!("https://github.com/{login}"),
            r#type: "User".to_string(),
            user_view_type: None,
            site_admin: false,
            name: None,
            email: None,
        }
    }

    fn user(id: u64) -> User {
        User {
            login: format!("user{id}"),
            id,
            node_id: None,
            avatar_url: "https://avatars.example/a".to_string(),
            gravatar_id: None,
            html_url: format!("https://github.com/user{id}"),
            r#type: "User".to_string(),
            user_view_type: None,
            site_admin: false,
            name: Some("User".to_string()),
            company: None,
            blog: None,
            location: None,
            email: None,
            hireable: None,
            bio: None,
            twitter_username: None,
            public_repos: 0,
            public_gists: 0,
            followers: 0,
            following: 0,
            created_at: "2020-01-01T00:00:00Z".to_string(),
            updated_at: "2020-01-01T00:00:00Z".to_string(),
        }
    }

    fn repo(id: u64, full_name: &str) -> Repo {
        let name = full_name.rsplit('/').next().unwrap().to_string();
        Repo {
            id,
            node_id: None,
            name,
            full_name: full_name.to_string(),
            owner: simple_user(id, "owner"),
            private: false,
            html_url: format!("https://github.com/{full_name}"),
            description: None,
            fork: false,
            created_at: "2020-01-01T00:00:00Z".to_string(),
            updated_at: "2020-01-01T00:00:00Z".to_string(),
            pushed_at: None,
            size: 0,
            stargazers_count: 10,
            watchers_count: 10,
            language: Some("Rust".to_string()),
            forks_count: 1,
            open_issues_count: 0,
            mirror_url: None,
            archived: false,
            disabled: false,
            license: Some(License {
                key: "mit".to_string(),
                name: "MIT License".to_string(),
                spdx_id: Some("MIT".to_string()),
                node_id: None,
            }),
            allow_forking: true,
            is_template: false,
            web_commit_signoff_required: false,
            topics: vec!["rust".to_string()],
            visibility: "public".to_string(),
            forks: 1,
            open_issues: 0,
            watchers: 10,
            default_branch: "main".to_string(),
            organization: None,
            network_count: 1,
            subscribers_count: 1,
            has_issues: true,
            has_projects: true,
            has_downloads: true,
            has_wiki: true,
            has_pages: false,
            has_discussions: false,
            has_pull_requests: true,
            pull_request_creation_policy: None,
        }
    }

    fn readme() -> Readme {
        Readme {
            name: "README.md".to_string(),
            path: "README.md".to_string(),
            sha: "abc".to_string(),
            size: 11,
            html_url: "https://github.com/acme/widget/blob/main/README.md".to_string(),
            r#type: "file".to_string(),
            content: "aGVsbG8gd29ybGQ=".to_string(),
            encoding: "base64".to_string(),
        }
    }

    #[tokio::test]
    async fn upserts_user() {
        let db = test_db().await;
        db.upsert_user(&user(1)).await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM github_users")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(count, 1);

        let login: String = sqlx::query_scalar("SELECT login FROM github_users WHERE id = ?")
            .bind(1)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(login, "user1");

        let mut updated = user(1);
        updated.login = "renamed".to_string();
        db.upsert_user(&updated).await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM github_users")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "upsert must not insert a duplicate row");

        let login: String = sqlx::query_scalar("SELECT login FROM github_users WHERE id = ?")
            .bind(1)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(login, "renamed");
    }

    #[tokio::test]
    async fn upserts_repo_with_owner() {
        let db = test_db().await;
        db.upsert_repo(&repo(7, "acme/widget")).await.unwrap();

        let repo_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM github_repos")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(repo_count, 1);

        let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM github_users")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(user_count, 1, "owner must be stored as a user");

        let owner_id: Option<i64> =
            sqlx::query_scalar("SELECT owner_id FROM github_repos WHERE id = ?")
                .bind(7)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(owner_id, Some(7));
    }

    #[tokio::test]
    async fn inserts_trending() {
        let db = test_db().await;
        db.ensure_repo(3, "acme/widget").await.unwrap();
        let params = serde_json::json!({"language": "rust", "period": "weekly"});

        db.insert_trending(
            "github/trending",
            Some(3),
            Some("acme/widget"),
            Some(&params),
            100,
        )
        .await
        .unwrap();
        db.insert_trending(
            "huggingface/hot",
            None,
            Some("org/model"),
            Some(&params),
            100,
        )
        .await
        .unwrap();
        db.insert_trending(
            "github/trending",
            Some(3),
            Some("acme/widget"),
            Some(&params),
            100,
        )
        .await
        .unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM github_trendings")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(
            count, 2,
            "identical observation in the same batch must not duplicate"
        );

        let err = db
            .insert_trending("github/search", None, None, None, 200)
            .await;
        assert!(err.is_err(), "repo_id and full_name cannot both be empty");
    }

    #[tokio::test]
    async fn upserts_star_history() {
        let db = test_db().await;
        db.ensure_repo(5, "acme/widget").await.unwrap();
        let weeks = [StargazerHistory {
            week: 1000,
            total: 10,
            days: vec![1, 2, 3, 4, 5, 6, 7],
        }];

        db.insert_star_history(5, &weeks, 100).await.unwrap();
        db.insert_star_history(
            5,
            &[StargazerHistory {
                week: 1000,
                total: 42,
                days: vec![1, 2, 3, 4, 5, 6, 7],
            }],
            100,
        )
        .await
        .unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM github_star_history")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(
            count, 1,
            "current-only history must overwrite, not accumulate"
        );

        let total: i64 = sqlx::query_scalar(
            "SELECT total FROM github_star_history WHERE repo_id = ? AND week = ?",
        )
        .bind(5)
        .bind(1000)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(total, 42);
    }

    #[tokio::test]
    async fn upserts_readme() {
        let db = test_db().await;
        db.ensure_repo(9, "acme/widget").await.unwrap();
        db.upsert_readme(9, &readme()).await.unwrap();

        let content: String =
            sqlx::query_scalar("SELECT content FROM github_readmes WHERE repo_id = ?")
                .bind(9)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(content, "hello world", "base64 content must be decoded");

        let mut updated = readme();
        updated.content = "c2Vjb25k".to_string();
        updated.name = "README-2.md".to_string();
        db.upsert_readme(9, &updated).await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM github_readmes")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "upsert must not insert a duplicate row");

        let content: String =
            sqlx::query_scalar("SELECT content FROM github_readmes WHERE repo_id = ?")
                .bind(9)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(content, "second");
    }

    #[tokio::test]
    async fn workflow_lifecycle() {
        let db = test_db().await;
        let items = [
            WorkflowItem {
                task: "trending:rust:weekly".to_string(),
                params: "{\"language\":\"rust\",\"period\":\"weekly\"}".to_string(),
                status: "pending".to_string(),
            },
            WorkflowItem {
                task: "search:rust".to_string(),
                params: "{\"language\":\"rust\"}".to_string(),
                status: "pending".to_string(),
            },
        ];

        let crawl = db
            .create_workflow("github-trending", 1000, &items)
            .await
            .unwrap();
        assert_eq!(crawl.status, "running");
        assert_eq!(crawl.created_at, 1000);

        let last = db.last_workflow("github-trending").await.unwrap().unwrap();
        assert_eq!(last.id, crawl.id);
        assert_eq!(last.status, "running");

        let pending = db.pending_workflow_items(crawl.id).await.unwrap();
        assert_eq!(pending.len(), 2);

        db.mark_workflow_item_done(crawl.id, "trending:rust:weekly")
            .await
            .unwrap();
        let pending = db.pending_workflow_items(crawl.id).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].task, "search:rust");

        db.ensure_workflow_items(crawl.id, &items).await.unwrap();
        let all: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM github_workflow_items WHERE workflow_id = ?")
                .bind(crawl.id)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(all, 2, "re-ensuring items must not duplicate");

        db.mark_workflow_done(crawl.id).await.unwrap();
        let last = db.last_workflow("github-trending").await.unwrap().unwrap();
        assert_eq!(last.status, "done");
    }

    #[tokio::test]
    async fn staleness_getters() {
        let db = test_db().await;
        assert!(db.repo_updated_at(7).await.unwrap().is_none());
        assert!(db.readme_updated_at(7).await.unwrap().is_none());
        assert!(db.star_history_updated_at(7).await.unwrap().is_none());
        assert!(db.repo_owner(7).await.unwrap().is_none());

        db.upsert_repo(&repo(7, "acme/widget")).await.unwrap();
        assert!(db.repo_updated_at(7).await.unwrap().is_some());
        assert_eq!(
            db.repo_owner(7).await.unwrap(),
            Some((7, "owner".to_string()))
        );
        assert!(
            db.user_full_updated_at(7).await.unwrap().is_none(),
            "owner upserted as SimpleUser must not set full_updated_at"
        );

        db.upsert_user(&user(7)).await.unwrap();
        assert!(
            db.user_full_updated_at(7).await.unwrap().is_some(),
            "full user upsert must set full_updated_at"
        );

        db.upsert_readme(7, &readme()).await.unwrap();
        assert!(db.readme_updated_at(7).await.unwrap().is_some());

        db.insert_star_history(
            7,
            &[StargazerHistory {
                week: 1000,
                total: 42,
                days: vec![1, 2, 3, 4, 5, 6, 7],
            }],
            100,
        )
        .await
        .unwrap();
        assert!(db.star_history_updated_at(7).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn ensure_repo_does_not_refresh() {
        let db = test_db().await;
        db.ensure_repo(7, "acme/widget").await.unwrap();
        let first: i64 = sqlx::query_scalar("SELECT updated_at FROM github_repos WHERE id = ?")
            .bind(7)
            .fetch_one(&db.pool)
            .await
            .unwrap();

        db.ensure_repo(7, "acme/widget").await.unwrap();
        let second: i64 = sqlx::query_scalar("SELECT updated_at FROM github_repos WHERE id = ?")
            .bind(7)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(first, second, "ensure_repo must not fake freshness");
    }
}
