use crate::event::{TicketEvent, TicketStatus};
use crate::id::{generate_id, resolve_prefix, PrefixError};
use crate::repo::{
    append_note_line, ensure_merge_strategy, list_pointer_ids, merge_base, read_note,
    resolve_base_branch, resolve_pointer_ref, set_pointer_ref, PointerKind, TICKETS_NOTES_REF,
};
use crate::ticket::{project_ticket, TicketState};
use git2::{Oid, Repository};

#[derive(Debug)]
pub enum TicketError {
    Git(git2::Error),
    DetachedHead,
    NotFound,
    Ambiguous(Vec<String>),
}

impl From<git2::Error> for TicketError {
    fn from(e: git2::Error) -> Self {
        TicketError::Git(e)
    }
}

fn resolve_id(repo: &Repository, id_prefix: &str) -> Result<String, TicketError> {
    let ids = list_pointer_ids(repo, PointerKind::Ticket);
    resolve_prefix(id_prefix, &ids).map_err(|e| match e {
        PrefixError::NotFound => TicketError::NotFound,
        PrefixError::Ambiguous(matches) => TicketError::Ambiguous(matches),
    })
}

fn events_for_root(repo: &Repository, root: Oid) -> Vec<TicketEvent> {
    read_note(repo, TICKETS_NOTES_REF, root)
        .map(|content| content.lines().filter_map(TicketEvent::from_line).collect())
        .unwrap_or_default()
}

fn load_ticket(repo: &Repository, id: &str) -> Result<TicketState, TicketError> {
    let root = resolve_pointer_ref(repo, PointerKind::Ticket, id).ok_or(TicketError::NotFound)?;
    project_ticket(id, &events_for_root(repo, root)).ok_or(TicketError::NotFound)
}

fn append_event(repo: &Repository, id: &str, event: &TicketEvent) -> Result<(), TicketError> {
    let root = resolve_pointer_ref(repo, PointerKind::Ticket, id).ok_or(TicketError::NotFound)?;
    ensure_merge_strategy(repo, TICKETS_NOTES_REF)?;
    append_note_line(repo, TICKETS_NOTES_REF, root, &event.to_line())?;
    Ok(())
}

pub fn create_ticket(
    repo: &Repository,
    base_branch: Option<&str>,
    title: &str,
    body: &str,
    assignee: Option<&str>,
    author: &str,
    ts: u64,
) -> Result<TicketState, TicketError> {
    let head = repo.head()?;
    let branch = head.shorthand().ok_or(TicketError::DetachedHead)?.to_string();
    let tip = head.peel_to_commit()?.id();
    let base_branch = resolve_base_branch(repo, base_branch);

    let root = match repo
        .find_branch(&base_branch, git2::BranchType::Local)
        .ok()
        .and_then(|b| b.get().target())
    {
        Some(base_oid) if base_oid != tip => merge_base(repo, tip, base_oid).unwrap_or(tip),
        _ => tip,
    };

    let id = generate_id();
    let event = TicketEvent::TicketCreated {
        id: id.clone(),
        title: title.to_string(),
        body: body.to_string(),
        branch,
        author: author.to_string(),
        ts,
    };

    ensure_merge_strategy(repo, TICKETS_NOTES_REF)?;
    append_note_line(repo, TICKETS_NOTES_REF, root, &event.to_line())?;
    set_pointer_ref(repo, PointerKind::Ticket, &id, root)?;

    if let Some(assignee) = assignee {
        assign_ticket(repo, &id, assignee, ts)?;
    }

    load_ticket(repo, &id)
}

pub fn list_tickets(repo: &Repository) -> Result<Vec<TicketState>, TicketError> {
    list_pointer_ids(repo, PointerKind::Ticket)
        .iter()
        .map(|id| load_ticket(repo, id))
        .collect()
}

pub fn show_ticket(repo: &Repository, id_prefix: &str) -> Result<TicketState, TicketError> {
    let id = resolve_id(repo, id_prefix)?;
    load_ticket(repo, &id)
}

pub fn set_status(repo: &Repository, id_prefix: &str, status: TicketStatus, ts: u64) -> Result<TicketState, TicketError> {
    let id = resolve_id(repo, id_prefix)?;
    append_event(repo, &id, &TicketEvent::StatusChanged { id: id.clone(), status, ts })?;
    load_ticket(repo, &id)
}

pub fn assign_ticket(repo: &Repository, id_prefix: &str, assignee: &str, ts: u64) -> Result<TicketState, TicketError> {
    let id = resolve_id(repo, id_prefix)?;
    append_event(repo, &id, &TicketEvent::Assigned { id: id.clone(), assignee: assignee.to_string(), ts })?;
    load_ticket(repo, &id)
}

pub fn comment_ticket(repo: &Repository, id_prefix: &str, body: &str, author: &str, ts: u64) -> Result<TicketState, TicketError> {
    let id = resolve_id(repo, id_prefix)?;
    append_event(repo, &id, &TicketEvent::TicketCommented { id: id.clone(), body: body.to_string(), author: author.to_string(), ts })?;
    load_ticket(repo, &id)
}
