use crate::event::{ReviewEvent, TicketEvent};
use crate::repo::{
    list_pointer_ids, pointer_ref_name, read_note, resolve_pointer_ref, PointerKind,
    REVIEWS_NOTES_REF, TICKETS_NOTES_REF,
};
use git2::Repository;

#[derive(Debug, Clone, PartialEq)]
pub struct Orphan {
    pub kind: PointerKind,
    pub id: String,
}

fn ticket_note_has_id(repo: &Repository, notes_ref: &str, commit: git2::Oid, id: &str) -> bool {
    read_note(repo, notes_ref, commit)
        .map(|content| content.lines().filter_map(TicketEvent::from_line).any(|e| e.id() == id))
        .unwrap_or(false)
}

fn review_note_has_id(repo: &Repository, notes_ref: &str, commit: git2::Oid, id: &str) -> bool {
    read_note(repo, notes_ref, commit)
        .map(|content| content.lines().filter_map(ReviewEvent::from_line).any(|e| e.id() == id))
        .unwrap_or(false)
}

pub fn find_orphaned_pointers(repo: &Repository) -> Vec<Orphan> {
    let mut orphans = Vec::new();

    for id in list_pointer_ids(repo, PointerKind::Ticket) {
        match resolve_pointer_ref(repo, PointerKind::Ticket, &id) {
            Some(commit) if ticket_note_has_id(repo, TICKETS_NOTES_REF, commit, &id) => {}
            _ => orphans.push(Orphan { kind: PointerKind::Ticket, id }),
        }
    }

    for id in list_pointer_ids(repo, PointerKind::Review) {
        match resolve_pointer_ref(repo, PointerKind::Review, &id) {
            Some(commit) if review_note_has_id(repo, REVIEWS_NOTES_REF, commit, &id) => {}
            _ => orphans.push(Orphan { kind: PointerKind::Review, id }),
        }
    }

    orphans
}

pub fn prune_orphan(repo: &Repository, orphan: &Orphan) -> Result<(), git2::Error> {
    let name = pointer_ref_name(orphan.kind, &orphan.id);
    let mut reference = repo.find_reference(&name)?;
    reference.delete()
}
