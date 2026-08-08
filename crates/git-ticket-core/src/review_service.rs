use crate::event::{ReviewEvent, Verdict};
use crate::id::{generate_id, resolve_prefix, PrefixError};
use crate::repo::{
    append_note_line, ensure_merge_strategy, list_pointer_ids, merge_base, read_note,
    resolve_base_branch, resolve_pointer_ref, set_pointer_ref, PointerKind, REVIEWS_NOTES_REF,
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
    let base_name = resolve_base_branch(repo, base);
    // Resolved eagerly so opening a review against a non-existent base fails
    // immediately rather than at display time.
    let _base_oid = resolve_commitish(repo, &base_name)?;

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

    load_review(repo, &id)
}

/// Resolve the `(base, target)` commit pair a review's diff must be computed
/// over.
///
/// The target is taken from the review's pointer ref
/// (`refs/git-ticket/reviews/<id>`), which was set to the commit the branch
/// pointed at when the review was opened and is never rewritten -- so commits
/// pushed to that branch *after* the review was opened do not silently leak
/// into the diff. The base is the merge-base of that snapshotted target with
/// the *current* tip of the review's base branch, matching the spec's "diff
/// base for a branch review: merge-base of the branch with a configured base
/// branch". Resolving the base branch by name (rather than snapshotting it)
/// is intentional: it is a moving ref by design, and taking the merge-base
/// keeps unrelated base-branch commits out of the diff.
///
/// Both the CLI `review show` and the web review detail page go through this
/// so the two renderings can never diverge.
pub fn review_diff_range(repo: &Repository, review: &ReviewState) -> Result<(Oid, Oid), ReviewError> {
    let target_oid = resolve_pointer_ref(repo, PointerKind::Review, &review.id)
        .ok_or(ReviewError::NotFound)?;
    let base_tip = resolve_commitish(repo, &review.base)?;
    let base_oid = merge_base(repo, target_oid, base_tip).unwrap_or(base_tip);
    Ok((base_oid, target_oid))
}

// Mirrors the CLI's flat `review comment` arg surface (file, line, body,
// reply_to, author, ts) 1:1 by design; grouping into a struct would just
// move the same fields around rather than reduce them.
#[allow(clippy::too_many_arguments)]
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
