use crate::event::{ReviewEvent, Verdict};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReviewComment {
    pub thread_id: String,
    pub parent_id: Option<String>,
    pub file: String,
    pub line: u32,
    pub body: String,
    pub author: String,
    pub ts: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReviewState {
    pub id: String,
    pub target: String,
    pub base: String,
    pub author: String,
    pub opened_ts: u64,
    pub comments: Vec<ReviewComment>,
    pub verdicts: Vec<(String, Verdict, u64)>,
}

impl ReviewState {
    pub fn latest_verdict(&self) -> Option<&(String, Verdict, u64)> {
        self.verdicts.last()
    }
}

/// Tie-break priority for events sharing a timestamp: `ReviewOpened` must
/// always be applied first, since it establishes the base state that every
/// other event mutates. Everything else is equal priority.
fn event_priority(e: &ReviewEvent) -> u8 {
    matches!(e, ReviewEvent::ReviewOpened { .. }).then_some(0).unwrap_or(1)
}

pub fn project_review(id: &str, events: &[ReviewEvent]) -> Option<ReviewState> {
    let mut relevant: Vec<&ReviewEvent> = events.iter().filter(|e| e.id() == id).collect();
    // Sort purely by each event's own content, never by input array order:
    // after a cross-clone sync, `log::merge_cat_sort_uniq` resorts an entire
    // note's lines into lexicographic string order, so "input order" can no
    // longer be relied on to reflect chronological append order. Primary key
    // is timestamp; secondary key ensures `ReviewOpened` always applies
    // before any other same-timestamp event; tertiary key is a deterministic
    // lexicographic tie-break for same-timestamp, same-priority events.
    relevant.sort_by(|a, b| {
        a.ts()
            .cmp(&b.ts())
            .then_with(|| event_priority(a).cmp(&event_priority(b)))
            .then_with(|| a.to_line().cmp(&b.to_line()))
    });

    let mut state: Option<ReviewState> = None;
    for event in relevant {
        match event {
            ReviewEvent::ReviewOpened { id, target, base, author, ts } => {
                state = Some(ReviewState {
                    id: id.clone(),
                    target: target.clone(),
                    base: base.clone(),
                    author: author.clone(),
                    opened_ts: *ts,
                    comments: Vec::new(),
                    verdicts: Vec::new(),
                });
            }
            ReviewEvent::CommentAdded { file, line, thread_id, parent_id, body, author, ts, .. } => {
                if let Some(s) = state.as_mut() {
                    s.comments.push(ReviewComment {
                        thread_id: thread_id.clone(),
                        parent_id: parent_id.clone(),
                        file: file.clone(),
                        line: *line,
                        body: body.clone(),
                        author: author.clone(),
                        ts: *ts,
                    });
                }
            }
            ReviewEvent::VerdictSet { verdict, author, ts, .. } => {
                if let Some(s) = state.as_mut() {
                    s.verdicts.push((author.clone(), verdict.clone(), *ts));
                }
            }
        }
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{ReviewEvent, Verdict};

    fn opened(id: &str, ts: u64) -> ReviewEvent {
        ReviewEvent::ReviewOpened { id: id.into(), target: "abc123".into(), base: "main".into(), author: "alex".into(), ts }
    }

    #[test]
    fn projecting_unknown_id_returns_none() {
        assert_eq!(project_review("nope", &[]), None);
    }

    #[test]
    fn opened_event_alone_yields_review_with_no_comments_or_verdict() {
        let events = vec![opened("r1", 1)];
        let state = project_review("r1", &events).unwrap();
        assert_eq!(state.target, "abc123");
        assert_eq!(state.base, "main");
        assert!(state.comments.is_empty());
        assert!(state.latest_verdict().is_none());
    }

    #[test]
    fn same_timestamp_opened_and_verdict_still_apply_opened_first() {
        // Simulates the state of a note's lines after `log::merge_cat_sort_uniq`
        // resorts them lexicographically on cross-clone sync: a `VerdictSet`
        // line can land before a same-timestamp `ReviewOpened` line purely
        // because "VerdictSet" < "ReviewOpened" is not guaranteed by any
        // chronological ordering.
        let events = vec![
            ReviewEvent::VerdictSet { id: "r1".into(), verdict: Verdict::Approve, author: "alex".into(), ts: 5 },
            opened("r1", 5),
        ];
        let state = project_review("r1", &events).unwrap();
        let (author, verdict, ts) = state.latest_verdict().unwrap();
        assert_eq!(author, "alex");
        assert_eq!(*verdict, Verdict::Approve);
        assert_eq!(*ts, 5);
    }

    #[test]
    fn comments_accumulate_in_timestamp_order() {
        let events = vec![
            opened("r1", 1),
            ReviewEvent::CommentAdded {
                id: "r1".into(), file: "src/lib.rs".into(), line: 10, thread_id: "t1".into(),
                parent_id: None, body: "second".into(), author: "bob".into(), ts: 3,
            },
            ReviewEvent::CommentAdded {
                id: "r1".into(), file: "src/lib.rs".into(), line: 5, thread_id: "t2".into(),
                parent_id: None, body: "first".into(), author: "alex".into(), ts: 2,
            },
        ];
        let state = project_review("r1", &events).unwrap();
        let bodies: Vec<&str> = state.comments.iter().map(|c| c.body.as_str()).collect();
        assert_eq!(bodies, vec!["first", "second"]);
    }

    #[test]
    fn latest_verdict_wins_when_author_changes_their_mind() {
        let events = vec![
            opened("r1", 1),
            ReviewEvent::VerdictSet { id: "r1".into(), verdict: Verdict::RequestChanges, author: "alex".into(), ts: 2 },
            ReviewEvent::VerdictSet { id: "r1".into(), verdict: Verdict::Approve, author: "alex".into(), ts: 3 },
        ];
        let state = project_review("r1", &events).unwrap();
        let (author, verdict, ts) = state.latest_verdict().unwrap();
        assert_eq!(author, "alex");
        assert_eq!(*verdict, Verdict::Approve);
        assert_eq!(*ts, 3);
    }

    #[test]
    fn events_for_other_ids_are_ignored() {
        let events = vec![opened("r1", 1), opened("r2", 1)];
        let state = project_review("r1", &events).unwrap();
        assert_eq!(state.id, "r1");
    }

    #[test]
    fn review_state_serializes_to_json_with_expected_keys() {
        let events = vec![
            opened("r1", 1),
            ReviewEvent::VerdictSet { id: "r1".into(), verdict: Verdict::Approve, author: "alex".into(), ts: 2 },
        ];
        let state = project_review("r1", &events).unwrap();
        let json = serde_json::to_string(&state).unwrap();
        for key in ["id", "target", "base", "author", "opened_ts", "comments", "verdicts"] {
            assert!(json.contains(&format!("\"{key}\"")), "missing key '{key}' in {json}");
        }
        assert!(json.contains("\"approve\""));
    }
}
