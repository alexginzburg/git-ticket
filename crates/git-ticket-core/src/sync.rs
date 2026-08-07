use crate::repo::{
    list_pointer_ids, merge_note, read_note, resolve_pointer_ref, set_pointer_ref, PointerKind,
    REVIEWS_NOTES_REF, TICKETS_NOTES_REF,
};
use git2::{Oid, Remote, Repository};

#[derive(Debug, Default)]
pub struct SyncReport {
    pub tickets_merged: usize,
    pub reviews_merged: usize,
}

const FETCH_TICKETS_NOTES: &str = "refs/git-ticket-fetch/notes/tickets";
const FETCH_REVIEWS_NOTES: &str = "refs/git-ticket-fetch/notes/reviews";
const FETCH_TICKET_POINTERS: &str = "refs/git-ticket-fetch/tickets/*";
const FETCH_REVIEW_POINTERS: &str = "refs/git-ticket-fetch/reviews/*";

fn fetch_refspecs() -> Vec<String> {
    vec![
        format!("{TICKETS_NOTES_REF}:{FETCH_TICKETS_NOTES}"),
        format!("{REVIEWS_NOTES_REF}:{FETCH_REVIEWS_NOTES}"),
        format!("refs/git-ticket/tickets/*:{FETCH_TICKET_POINTERS}"),
        format!("refs/git-ticket/reviews/*:{FETCH_REVIEW_POINTERS}"),
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

pub fn sync(repo: &Repository, remote_name: &str) -> Result<SyncReport, git2::Error> {
    let mut remote: Remote = repo.find_remote(remote_name)?;

    let specs = fetch_refspecs();
    let spec_refs: Vec<&str> = specs.iter().map(String::as_str).collect();
    remote.fetch(&spec_refs, None, None)?;

    adopt_fetched_pointer_refs(repo, PointerKind::Ticket, "refs/git-ticket-fetch/tickets/")?;
    adopt_fetched_pointer_refs(repo, PointerKind::Review, "refs/git-ticket-fetch/reviews/")?;

    let tickets_merged = merge_notes_ref(repo, TICKETS_NOTES_REF, FETCH_TICKETS_NOTES)?;
    let reviews_merged = merge_notes_ref(repo, REVIEWS_NOTES_REF, FETCH_REVIEWS_NOTES)?;

    let ticket_ids = list_pointer_ids(repo, PointerKind::Ticket);
    let review_ids = list_pointer_ids(repo, PointerKind::Review);
    let mut push_specs = vec![
        format!("{TICKETS_NOTES_REF}:{TICKETS_NOTES_REF}"),
        format!("{REVIEWS_NOTES_REF}:{REVIEWS_NOTES_REF}"),
    ];
    for id in &ticket_ids {
        push_specs.push(format!("refs/git-ticket/tickets/{id}:refs/git-ticket/tickets/{id}"));
    }
    for id in &review_ids {
        push_specs.push(format!("refs/git-ticket/reviews/{id}:refs/git-ticket/reviews/{id}"));
    }
    // Only push refspecs whose source ref actually exists locally -- a
    // repo that has never created a ticket/review has no notes ref yet,
    // and libgit2 rejects the whole push if any single refspec's source
    // is missing.
    push_specs.retain(|spec| {
        let src = spec.split(':').next().unwrap_or("");
        repo.find_reference(src).is_ok()
    });
    if !push_specs.is_empty() {
        let push_refs: Vec<&str> = push_specs.iter().map(String::as_str).collect();
        remote.push(&push_refs, None)?;
    }

    Ok(SyncReport { tickets_merged, reviews_merged })
}
