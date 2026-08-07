use git2::Repository;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn open_repo() -> Result<Repository, String> {
    Repository::discover(".").map_err(|e| format!("not a git repository: {e}"))
}

pub fn current_author(repo: &Repository) -> String {
    repo.config()
        .ok()
        .and_then(|c| c.get_string("user.name").ok())
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn now_ts() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}
