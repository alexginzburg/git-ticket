use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TicketStatus {
    Open,
    InProgress,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TicketEvent {
    TicketCreated { id: String, title: String, body: String, branch: String, author: String, ts: u64 },
    StatusChanged { id: String, status: TicketStatus, ts: u64 },
    Assigned { id: String, assignee: String, ts: u64 },
    TicketCommented { id: String, body: String, author: String, ts: u64 },
}

impl TicketEvent {
    pub fn id(&self) -> &str {
        match self {
            TicketEvent::TicketCreated { id, .. }
            | TicketEvent::StatusChanged { id, .. }
            | TicketEvent::Assigned { id, .. }
            | TicketEvent::TicketCommented { id, .. } => id,
        }
    }

    pub fn ts(&self) -> u64 {
        match self {
            TicketEvent::TicketCreated { ts, .. }
            | TicketEvent::StatusChanged { ts, .. }
            | TicketEvent::Assigned { ts, .. }
            | TicketEvent::TicketCommented { ts, .. } => *ts,
        }
    }

    pub fn to_line(&self) -> String {
        serde_json::to_string(self).expect("TicketEvent always serializes")
    }

    pub fn from_line(line: &str) -> Option<TicketEvent> {
        serde_json::from_str(line).ok()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    Approve,
    RequestChanges,
    Comment,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ReviewEvent {
    ReviewOpened { id: String, target: String, base: String, author: String, ts: u64 },
    CommentAdded {
        id: String,
        file: String,
        line: u32,
        thread_id: String,
        parent_id: Option<String>,
        body: String,
        author: String,
        ts: u64,
    },
    VerdictSet { id: String, verdict: Verdict, author: String, ts: u64 },
}

impl ReviewEvent {
    pub fn id(&self) -> &str {
        match self {
            ReviewEvent::ReviewOpened { id, .. }
            | ReviewEvent::CommentAdded { id, .. }
            | ReviewEvent::VerdictSet { id, .. } => id,
        }
    }

    pub fn ts(&self) -> u64 {
        match self {
            ReviewEvent::ReviewOpened { ts, .. }
            | ReviewEvent::CommentAdded { ts, .. }
            | ReviewEvent::VerdictSet { ts, .. } => *ts,
        }
    }

    pub fn to_line(&self) -> String {
        serde_json::to_string(self).expect("ReviewEvent always serializes")
    }

    pub fn from_line(line: &str) -> Option<ReviewEvent> {
        serde_json::from_str(line).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_event_roundtrips_through_a_line() {
        let event = TicketEvent::TicketCreated {
            id: "abc123".into(),
            title: "Fix login bug".into(),
            body: "Users can't log in on Safari".into(),
            branch: "fix/login".into(),
            author: "alex".into(),
            ts: 1_700_000_000,
        };
        let line = event.to_line();
        assert!(!line.contains('\n'));
        let parsed = TicketEvent::from_line(&line).expect("parses back");
        assert_eq!(parsed, event);
    }

    #[test]
    fn ticket_event_id_and_ts_accessors_cover_every_variant() {
        let events = vec![
            TicketEvent::TicketCreated { id: "a".into(), title: "t".into(), body: "b".into(), branch: "br".into(), author: "au".into(), ts: 1 },
            TicketEvent::StatusChanged { id: "a".into(), status: TicketStatus::Closed, ts: 2 },
            TicketEvent::Assigned { id: "a".into(), assignee: "bob".into(), ts: 3 },
            TicketEvent::TicketCommented { id: "a".into(), body: "hi".into(), author: "au".into(), ts: 4 },
        ];
        for event in &events {
            assert_eq!(event.id(), "a");
        }
        assert_eq!(events[0].ts(), 1);
        assert_eq!(events[3].ts(), 4);
    }

    #[test]
    fn review_event_roundtrips_through_a_line() {
        let event = ReviewEvent::VerdictSet {
            id: "rev1".into(),
            verdict: Verdict::RequestChanges,
            author: "alex".into(),
            ts: 42,
        };
        let line = event.to_line();
        let parsed = ReviewEvent::from_line(&line).expect("parses back");
        assert_eq!(parsed, event);
    }

    #[test]
    fn from_line_rejects_garbage() {
        assert!(TicketEvent::from_line("not json").is_none());
    }
}
