use crate::event::{ReviewEvent, Verdict};
use crate::id::{generate_id, resolve_prefix, PrefixError};
use crate::repo::{
    append_note_line, ensure_merge_strategy, list_pointer_ids, merge_base, read_note,
    resolve_pointer_ref, set_pointer_ref, PointerKind, REVIEWS_NOTES_REF,
};
use crate::review::{project_review, ReviewState};
use git2::{Oid, Repository};

#[derive(Debug)]
pub enum ReviewError {
    Git(git2::Error),
    NotFound,
    Ambiguous(Vec<String>),
    InvalidTarget(String),
}

impl From<git2::Error> for ReviewError {
    fn from(e: git2::Error) -> Self {
        ReviewError::Git(e)
    }
}

fn resolve_id(repo: &Repository, id_prefix: &str) -> Result<String, ReviewError> {
    let ids = list_pointer_ids(repo, PointerKind::Review);
    resolve_prefix(id_prefix, &ids).map_err(|e| match e {
        PrefixError::NotFound => ReviewError::NotFound,
        PrefixError::Ambiguous(matches) => ReviewError::Ambiguous(matches),
    })
}

fn events_for_commit(repo: &Repository, commit: Oid) -> Vec<ReviewEvent> {
    read_note(repo, REVIEWS_NOTES_REF, commit)
        .map(|content| content.lines().filter_map(ReviewEvent::from_line).collect())
        .unwrap_or_default()
}

fn load_review(repo: &Repository, id: &str) -> Result<ReviewState, ReviewError> {
    let commit = resolve_pointer_ref(repo, PointerKind::Review, id).ok_or(ReviewError::NotFound)?;
    project_review(id, &events_for_commit(repo, commit)).ok_or(ReviewError::NotFound)
}

fn append_event(repo: &Repository, id: &str, event: &ReviewEvent) -> Result<(), ReviewError> {
    let commit = resolve_pointer_ref(repo, PointerKind::Review, id).ok_or(ReviewError::NotFound)?;
    ensure_merge_strategy(repo, REVIEWS_NOTES_REF)?;
    append_note_line(repo, REVIEWS_NOTES_REF, commit, &event.to_line())?;
    Ok(())
}

fn resolve_commitish(repo: &Repository, name: &str) -> Result<Oid, ReviewError> {
    repo.revparse_single(name)
        .map(|obj| obj.id())
        .map_err(|_| ReviewError::InvalidTarget(name.to_string()))
}

pub fn start_review(
    repo: &Repository,
    target: &str,
    base: Option<&str>,
    author: &str,
    ts: u64,
) -> Result<ReviewState, ReviewError> {
    let target_oid = resolve_commitish(repo, target)?;
    let base_name = base
        .map(String::from)
        .or_else(|| {
            repo.config()
                .ok()
                .and_then(|c| c.get_string("ticket.baseBranch").ok())
        })
        .unwrap_or_else(|| "main".to_string());
    let base_oid = resolve_commitish(repo, &base_name)?;

    let id = generate_id();
    let event = ReviewEvent::ReviewOpened {
        id: id.clone(),
        target: target.to_string(),
        base: base_name,
        author: author.to_string(),
        ts,
    };

    ensure_merge_strategy(repo, REVIEWS_NOTES_REF)?;
    append_note_line(repo, REVIEWS_NOTES_REF, target_oid, &event.to_line())?;
    set_pointer_ref(repo, PointerKind::Review, &id, target_oid)?;
    let _ = merge_base; // merge_base is used by callers computing diffs (Task 11 web/CLI), kept imported for that purpose
    let _ = base_oid; // base is resolved eagerly to validate it exists before opening the review

    load_review(repo, &id)
}

pub fn add_comment(
    repo: &Repository,
    id_prefix: &str,
    file: &str,
    line: u32,
    body: &str,
    reply_to: Option<&str>,
    author: &str,
    ts: u64,
) -> Result<ReviewState, ReviewError> {
    let id = resolve_id(repo, id_prefix)?;
    let thread_id = reply_to.map(String::from).unwrap_or_else(generate_id);
    append_event(
        repo,
        &id,
        &ReviewEvent::CommentAdded {
            id: id.clone(),
            file: file.to_string(),
            line,
            thread_id,
            parent_id: reply_to.map(String::from),
            body: body.to_string(),
            author: author.to_string(),
            ts,
        },
    )?;
    load_review(repo, &id)
}

pub fn set_verdict(repo: &Repository, id_prefix: &str, verdict: Verdict, author: &str, ts: u64) -> Result<ReviewState, ReviewError> {
    let id = resolve_id(repo, id_prefix)?;
    append_event(repo, &id, &ReviewEvent::VerdictSet { id: id.clone(), verdict, author: author.to_string(), ts })?;
    load_review(repo, &id)
}

pub fn show_review(repo: &Repository, id_prefix: &str) -> Result<ReviewState, ReviewError> {
    let id = resolve_id(repo, id_prefix)?;
    load_review(repo, &id)
}
