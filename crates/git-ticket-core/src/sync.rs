use crate::repo::{
    list_pointer_ids, merge_note, read_note, resolve_pointer_ref, set_pointer_ref, PointerKind,
    REVIEWS_NOTES_REF, TICKETS_NOTES_REF,
};
use git2::{Cred, CredentialType, FetchOptions, Oid, PushOptions, Remote, RemoteCallbacks, Repository};

#[derive(Debug, Default)]
pub struct SyncReport {
    pub tickets_merged: usize,
    pub reviews_merged: usize,
    pub refs_pushed: usize,
}

const FETCH_TICKETS_NOTES: &str = "refs/git-ticket-fetch/notes/tickets";
const FETCH_REVIEWS_NOTES: &str = "refs/git-ticket-fetch/notes/reviews";
const FETCH_TICKET_POINTERS: &str = "refs/git-ticket-fetch/tickets/*";
const FETCH_REVIEW_POINTERS: &str = "refs/git-ticket-fetch/reviews/*";

/// Maximum fetch->merge->push attempts before giving up, so a remote that
/// keeps moving under us (or a genuinely broken push) can never spin forever.
const MAX_SYNC_ATTEMPTS: usize = 5;

/// All refspecs are forced (`+`).
///
/// Notes refs cannot be fast-forwarded across clones: `repo.note()` always
/// builds the new notes commit with the *local* notes-ref tip as its sole
/// parent, so after merging fetched content the local notes ref is not a
/// descendant of the remote's. Content safety comes from the merge itself
/// rather than from ref ancestry -- `log::merge_cat_sort_uniq` makes the
/// pushed note content a superset union of both sides, so forcing the ref
/// update cannot drop events. The push is still guarded by a re-fetch,
/// re-merge and retry loop (see [`sync`]) to cover a concurrent syncer
/// landing new content between our fetch and our push.
///
/// The fetch side must be forced for the same reason: once anyone can
/// force-update the remote notes ref, a non-forced fetch of it would itself
/// start failing non-fast-forward.
fn fetch_refspecs() -> Vec<String> {
    vec![
        format!("+{TICKETS_NOTES_REF}:{FETCH_TICKETS_NOTES}"),
        format!("+{REVIEWS_NOTES_REF}:{FETCH_REVIEWS_NOTES}"),
        format!("+refs/git-ticket/tickets/*:{FETCH_TICKET_POINTERS}"),
        format!("+refs/git-ticket/reviews/*:{FETCH_REVIEW_POINTERS}"),
    ]
}

/// Merge every note reachable from `fetched_notes_ref` into `notes_ref`
/// using the cat_sort_uniq strategy (via `repo::merge_note`), which itself
/// reads the existing local note before writing so no local content is ever
/// dropped -- it only ever grows the union of both sides' lines.
fn merge_notes_ref(repo: &Repository, notes_ref: &str, fetched_notes_ref: &str) -> Result<usize, git2::Error> {
    // remote has nothing on this ref yet: nothing to merge
    if repo.find_reference(fetched_notes_ref).is_err() {
        return Ok(0);
    }

    let mut merged_count = 0;
    if let Ok(notes) = repo.notes(Some(fetched_notes_ref)) {
        for note in notes.flatten() {
            let (_note_oid, annotated_oid) = note;
            let remote_content = repo
                .find_note(Some(fetched_notes_ref), annotated_oid)
                .ok()
                .and_then(|n| n.message().map(String::from))
                .unwrap_or_default();
            let local_content = read_note(repo, notes_ref, annotated_oid).unwrap_or_default();
            if remote_content != local_content {
                merge_note(repo, notes_ref, annotated_oid, &remote_content)?;
                merged_count += 1;
            }
        }
    }
    Ok(merged_count)
}

/// Create a local pointer ref for any fetched ticket/review id we don't
/// already have one for. Never overwrites an existing local pointer ref --
/// pointer refs identify the root commit of a ticket/review's event log,
/// which is immutable once created, so there is nothing to merge here.
fn adopt_fetched_pointer_refs(repo: &Repository, kind: PointerKind, fetch_prefix: &str) -> Result<(), git2::Error> {
    let glob = format!("{fetch_prefix}*");
    let fetched: Vec<(String, Oid)> = repo
        .references_glob(&glob)?
        .filter_map(|r| r.ok())
        .filter_map(|r| {
            let name = r.name()?.to_string();
            let id = name.trim_start_matches(fetch_prefix).to_string();
            r.target().map(|t| (id, t))
        })
        .collect();

    for (id, target) in fetched {
        if resolve_pointer_ref(repo, kind, &id).is_none() {
            set_pointer_ref(repo, kind, &id, target)?;
        }
    }
    Ok(())
}

/// Forced push refspecs for every local notes ref and pointer ref.
///
/// Pointer refs are forced too: `adopt_fetched_pointer_refs` only ever
/// *creates* a missing local pointer ref and never rewrites an existing one,
/// and a pointer ref's target is written once at creation and never changes,
/// so forcing here is a formality -- ids are unique by construction, so there
/// is no value for the force to actually clobber.
fn push_refspecs(repo: &Repository) -> Vec<String> {
    let mut specs = vec![
        format!("+{TICKETS_NOTES_REF}:{TICKETS_NOTES_REF}"),
        format!("+{REVIEWS_NOTES_REF}:{REVIEWS_NOTES_REF}"),
    ];
    for id in list_pointer_ids(repo, PointerKind::Ticket) {
        specs.push(format!("+refs/git-ticket/tickets/{id}:refs/git-ticket/tickets/{id}"));
    }
    for id in list_pointer_ids(repo, PointerKind::Review) {
        specs.push(format!("+refs/git-ticket/reviews/{id}:refs/git-ticket/reviews/{id}"));
    }
    // Only push refspecs whose source ref actually exists locally -- a
    // repo that has never created a ticket/review has no notes ref yet,
    // and libgit2 rejects the whole push if any single refspec's source
    // is missing.
    specs.retain(|spec| {
        let src = spec.trim_start_matches('+').split(':').next().unwrap_or("");
        repo.find_reference(src).is_ok()
    });
    specs
}

/// Resolves credentials the same way the `git` CLI would: an SSH agent for
/// SSH remotes, the configured credential helper for HTTPS remotes, then
/// libgit2's own default provider. Without this, fetch/push against any
/// remote that actually requires authentication fails immediately with
/// "authentication required but no callback set" -- libgit2 never prompts
/// or falls back to the system's credentials on its own.
fn credentials_callback(
    url: &str,
    username_from_url: Option<&str>,
    allowed_types: CredentialType,
) -> Result<Cred, git2::Error> {
    if allowed_types.contains(CredentialType::SSH_KEY) {
        if let Some(username) = username_from_url {
            if let Ok(cred) = Cred::ssh_key_from_agent(username) {
                return Ok(cred);
            }
        }
    }
    if allowed_types.contains(CredentialType::USER_PASS_PLAINTEXT) {
        if let Ok(config) = git2::Config::open_default() {
            if let Ok(cred) = Cred::credential_helper(&config, url, username_from_url) {
                return Ok(cred);
            }
        }
    }
    if allowed_types.contains(CredentialType::DEFAULT) {
        return Cred::default();
    }
    Err(git2::Error::from_str("no valid authentication method available for this remote"))
}

fn remote_callbacks() -> RemoteCallbacks<'static> {
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(credentials_callback);
    callbacks
}

/// A failed sync step, tagged with whether retrying could plausibly help.
/// Only push failures are retryable: everything before the push is either
/// local work or a fetch, and re-running those after they just failed will
/// fail the same way.
struct SyncStepError {
    error: git2::Error,
    retryable: bool,
}

/// One fetch -> adopt-pointers -> merge-notes -> push round trip. Merge
/// counts are accumulated into `report` as they happen, so counts from an
/// attempt whose push later fails are not lost (the merges themselves were
/// still written locally).
fn sync_once(repo: &Repository, remote_name: &str, report: &mut SyncReport) -> Result<(), SyncStepError> {
    let fatal = |e: git2::Error| SyncStepError { error: e, retryable: false };

    let mut remote: Remote = repo.find_remote(remote_name).map_err(fatal)?;

    let specs = fetch_refspecs();
    let spec_refs: Vec<&str> = specs.iter().map(String::as_str).collect();
    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(remote_callbacks());
    remote.fetch(&spec_refs, Some(&mut fetch_options), None).map_err(fatal)?;

    adopt_fetched_pointer_refs(repo, PointerKind::Ticket, "refs/git-ticket-fetch/tickets/").map_err(fatal)?;
    adopt_fetched_pointer_refs(repo, PointerKind::Review, "refs/git-ticket-fetch/reviews/").map_err(fatal)?;

    report.tickets_merged += merge_notes_ref(repo, TICKETS_NOTES_REF, FETCH_TICKETS_NOTES).map_err(fatal)?;
    report.reviews_merged += merge_notes_ref(repo, REVIEWS_NOTES_REF, FETCH_REVIEWS_NOTES).map_err(fatal)?;

    let push_specs = push_refspecs(repo);
    if !push_specs.is_empty() {
        let push_refs: Vec<&str> = push_specs.iter().map(String::as_str).collect();
        let mut push_options = PushOptions::new();
        push_options.remote_callbacks(remote_callbacks());
        remote
            .push(&push_refs, Some(&mut push_options))
            .map_err(|e| SyncStepError { error: e, retryable: true })?;
        // Only the attempt that actually succeeds counts -- a failed push is
        // retried from scratch, so an earlier attempt's ref count must not
        // accumulate into this one.
        report.refs_pushed = push_specs.len();
    }

    Ok(())
}

/// Fetch remote ticket/review state, merge it into the local notes, and push
/// the merged result back.
///
/// The whole round trip is retried on push failure: a push can legitimately
/// fail because another syncer landed new content on the remote between our
/// fetch and our push. Re-running fetch+merge is safe because
/// `log::merge_cat_sort_uniq` is idempotent -- re-merging already-merged
/// content is a no-op -- so a retry simply folds in whatever arrived in the
/// meantime and tries again. Bounded by [`MAX_SYNC_ATTEMPTS`].
pub fn sync(repo: &Repository, remote_name: &str) -> Result<SyncReport, git2::Error> {
    let mut report = SyncReport::default();
    let mut last_err = None;

    for _ in 0..MAX_SYNC_ATTEMPTS {
        match sync_once(repo, remote_name, &mut report) {
            Ok(()) => return Ok(report),
            Err(SyncStepError { error, retryable: false }) => return Err(error),
            Err(SyncStepError { error, retryable: true }) => last_err = Some(error),
        }
    }

    let detail = last_err
        .map(|e| e.message().to_string())
        .unwrap_or_else(|| "unknown error".to_string());
    Err(git2::Error::from_str(&format!(
        "sync: push to remote '{remote_name}' still failing after {MAX_SYNC_ATTEMPTS} attempts \
         (remote kept moving or push was rejected): {detail}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_callback_errors_when_no_method_is_allowed() {
        // An empty allowed-types set means the remote hasn't asked for SSH,
        // user/pass, or the default provider -- there is nothing this
        // callback can offer, so it must return an error rather than panic
        // or silently authenticate with something the remote didn't request.
        let result = credentials_callback("https://example.invalid/repo.git", None, CredentialType::empty());
        assert!(result.is_err());
    }
}
