pub mod github;
pub mod response;

pub use github::{
    Github, GithubBuilder, License, MAX_SEARCH_PAGES, Readme, Repo, RepoSearch, SearchOrder,
    SearchSort, SimpleUser, TrendingRepo, User,
};
pub use response::{SourceError, SourceResponse, SourceResult};
