use crate::event::{TicketEvent, TicketStatus, TicketType};
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
    pub ticket_type: TicketType,
    pub assignee: Option<String>,
    pub comments: Vec<Comment>,
}

/// Tie-break priority for events sharing a timestamp: `TicketCreated` must
/// always be applied first, since it establishes the base state that every
/// other event mutates. Everything else is equal priority.
fn event_priority(e: &TicketEvent) -> u8 {
    matches!(e, TicketEvent::TicketCreated { .. }).then_some(0).unwrap_or(1)
}

pub fn project_ticket(id: &str, events: &[TicketEvent]) -> Option<TicketState> {
    let mut relevant: Vec<&TicketEvent> = events.iter().filter(|e| e.id() == id).collect();
    // Sort purely by each event's own content, never by input array order:
    // after a cross-clone sync, `log::merge_cat_sort_uniq` resorts an entire
    // note's lines into lexicographic string order, so "input order" can no
    // longer be relied on to reflect chronological append order. Primary key
    // is timestamp; secondary key ensures `TicketCreated` always applies
    // before any other same-timestamp event (it establishes the base state
    // everything else applies on top of); tertiary key is a deterministic
    // lexicographic tie-break for same-timestamp, same-priority events.
    relevant.sort_by(|a, b| {
        a.ts()
            .cmp(&b.ts())
            .then_with(|| event_priority(a).cmp(&event_priority(b)))
            .then_with(|| a.to_line().cmp(&b.to_line()))
    });

    let mut state: Option<TicketState> = None;
    for event in relevant {
        match event {
            TicketEvent::TicketCreated { id, title, body, branch, author, ticket_type, ts } => {
                state = Some(TicketState {
                    id: id.clone(),
                    title: title.clone(),
                    body: body.clone(),
                    branch: branch.clone(),
                    author: author.clone(),
                    created_ts: *ts,
                    status: TicketStatus::Open,
                    ticket_type: ticket_type.clone(),
                    assignee: None,
                    comments: Vec::new(),
                });
            }
            TicketEvent::StatusChanged { status, .. } => {
                if let Some(s) = state.as_mut() {
                    s.status = status.clone();
                }
            }
            TicketEvent::TypeChanged { ticket_type, .. } => {
                if let Some(s) = state.as_mut() {
                    s.ticket_type = ticket_type.clone();
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
    use crate::event::{TicketEvent, TicketStatus, TicketType};

    fn created(id: &str, ts: u64) -> TicketEvent {
        TicketEvent::TicketCreated {
            id: id.into(), title: "Fix bug".into(), body: "desc".into(),
            branch: "fix/x".into(), author: "alex".into(), ticket_type: TicketType::Task, ts,
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
        assert_eq!(state.ticket_type, TicketType::Task);
        assert_eq!(state.assignee, None);
        assert!(state.comments.is_empty());
    }

    #[test]
    fn later_type_change_overrides_earlier_one() {
        let events = vec![
            created("a", 1),
            TicketEvent::TypeChanged { id: "a".into(), ticket_type: TicketType::Feature, ts: 2 },
            TicketEvent::TypeChanged { id: "a".into(), ticket_type: TicketType::Bug, ts: 3 },
        ];
        let state = project_ticket("a", &events).unwrap();
        assert_eq!(state.ticket_type, TicketType::Bug);
    }

    #[test]
    fn same_timestamp_created_and_type_changed_still_apply_created_first() {
        let events = vec![
            TicketEvent::TypeChanged { id: "a".into(), ticket_type: TicketType::Bug, ts: 5 },
            created("a", 5),
        ];
        let state = project_ticket("a", &events).unwrap();
        assert_eq!(state.ticket_type, TicketType::Bug);
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
    fn same_timestamp_created_and_status_changed_still_apply_created_first() {
        // Simulates the state of a note's lines after `log::merge_cat_sort_uniq`
        // resorts them lexicographically on cross-clone sync: a `StatusChanged`
        // line can land before a same-timestamp `TicketCreated` line purely
        // because "StatusChanged" < "TicketCreated" lexicographically.
        let events = vec![
            TicketEvent::StatusChanged { id: "a".into(), status: TicketStatus::Closed, ts: 5 },
            created("a", 5),
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
