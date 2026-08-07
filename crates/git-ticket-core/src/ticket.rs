use crate::event::{TicketEvent, TicketStatus};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Comment {
    pub body: String,
    pub author: String,
    pub ts: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TicketState {
    pub id: String,
    pub title: String,
    pub body: String,
    pub branch: String,
    pub author: String,
    pub created_ts: u64,
    pub status: TicketStatus,
    pub assignee: Option<String>,
    pub comments: Vec<Comment>,
}

pub fn project_ticket(id: &str, events: &[TicketEvent]) -> Option<TicketState> {
    let mut relevant: Vec<&TicketEvent> = events.iter().filter(|e| e.id() == id).collect();
    // Stable sort by timestamp only: events with equal timestamps (common
    // when several commands run within the same wall-clock second) keep
    // their original relative order, which reflects the order they were
    // appended to the note. Breaking ties by serialized content instead
    // would reorder events arbitrarily (e.g. a later `TicketCreated`
    // sorting after `StatusChanged`/`Assigned` purely by JSON text),
    // silently resetting ticket state.
    relevant.sort_by_key(|e| e.ts());

    let mut state: Option<TicketState> = None;
    for event in relevant {
        match event {
            TicketEvent::TicketCreated { id, title, body, branch, author, ts } => {
                state = Some(TicketState {
                    id: id.clone(),
                    title: title.clone(),
                    body: body.clone(),
                    branch: branch.clone(),
                    author: author.clone(),
                    created_ts: *ts,
                    status: TicketStatus::Open,
                    assignee: None,
                    comments: Vec::new(),
                });
            }
            TicketEvent::StatusChanged { status, .. } => {
                if let Some(s) = state.as_mut() {
                    s.status = status.clone();
                }
            }
            TicketEvent::Assigned { assignee, .. } => {
                if let Some(s) = state.as_mut() {
                    s.assignee = Some(assignee.clone());
                }
            }
            TicketEvent::TicketCommented { body, author, ts, .. } => {
                if let Some(s) = state.as_mut() {
                    s.comments.push(Comment { body: body.clone(), author: author.clone(), ts: *ts });
                }
            }
        }
    }
    state
}

pub fn project_all_tickets(events: &[TicketEvent]) -> HashMap<String, TicketState> {
    let mut ids: Vec<&str> = events.iter().map(|e| e.id()).collect();
    ids.sort();
    ids.dedup();
    ids.into_iter()
        .filter_map(|id| project_ticket(id, events).map(|s| (id.to_string(), s)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{TicketEvent, TicketStatus};

    fn created(id: &str, ts: u64) -> TicketEvent {
        TicketEvent::TicketCreated {
            id: id.into(), title: "Fix bug".into(), body: "desc".into(),
            branch: "fix/x".into(), author: "alex".into(), ts,
        }
    }

    #[test]
    fn projecting_unknown_id_returns_none() {
        assert_eq!(project_ticket("nope", &[]), None);
    }

    #[test]
    fn created_event_alone_yields_open_ticket_with_no_assignee() {
        let events = vec![created("a", 1)];
        let state = project_ticket("a", &events).expect("ticket exists");
        assert_eq!(state.title, "Fix bug");
        assert_eq!(state.status, TicketStatus::Open);
        assert_eq!(state.assignee, None);
        assert!(state.comments.is_empty());
    }

    #[test]
    fn later_status_change_overrides_earlier_one() {
        let events = vec![
            created("a", 1),
            TicketEvent::StatusChanged { id: "a".into(), status: TicketStatus::InProgress, ts: 2 },
            TicketEvent::StatusChanged { id: "a".into(), status: TicketStatus::Closed, ts: 3 },
        ];
        let state = project_ticket("a", &events).unwrap();
        assert_eq!(state.status, TicketStatus::Closed);
    }

    #[test]
    fn events_out_of_order_in_the_input_still_replay_by_timestamp() {
        let events = vec![
            TicketEvent::StatusChanged { id: "a".into(), status: TicketStatus::Closed, ts: 3 },
            created("a", 1),
            TicketEvent::StatusChanged { id: "a".into(), status: TicketStatus::InProgress, ts: 2 },
        ];
        let state = project_ticket("a", &events).unwrap();
        assert_eq!(state.status, TicketStatus::Closed);
    }

    #[test]
    fn comments_accumulate_in_timestamp_order() {
        let events = vec![
            created("a", 1),
            TicketEvent::TicketCommented { id: "a".into(), body: "second".into(), author: "bob".into(), ts: 3 },
            TicketEvent::TicketCommented { id: "a".into(), body: "first".into(), author: "alex".into(), ts: 2 },
        ];
        let state = project_ticket("a", &events).unwrap();
        let bodies: Vec<&str> = state.comments.iter().map(|c| c.body.as_str()).collect();
        assert_eq!(bodies, vec!["first", "second"]);
    }

    #[test]
    fn events_for_other_ids_are_ignored() {
        let events = vec![created("a", 1), created("b", 1)];
        let state = project_ticket("a", &events).unwrap();
        assert_eq!(state.id, "a");
    }

    #[test]
    fn project_all_tickets_returns_one_entry_per_id() {
        let events = vec![created("a", 1), created("b", 2)];
        let all = project_all_tickets(&events);
        assert_eq!(all.len(), 2);
        assert!(all.contains_key("a"));
        assert!(all.contains_key("b"));
    }
}
