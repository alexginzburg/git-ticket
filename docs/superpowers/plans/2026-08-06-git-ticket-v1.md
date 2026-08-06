# git-ticket v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `git-ticket`, a CLI (+ read-only local web viewer) that stores tickets and structured code reviews entirely inside git notes/refs, so they travel with the code over ordinary `push`/`fetch`/`clone` with no server and full offline support.

**Architecture:** A two-crate Cargo workspace. `git-ticket-core` holds all domain logic as pure, git2-backed functions: an append-only JSONL event log per git note, deterministic replay/projection into current state, and the git plumbing (notes, merge-base, pointer refs) needed to store it. `git-ticket-cli` is a thin `clap`-based CLI plus an embedded read-only `axum`/`askama` web server, both calling straight into `git-ticket-core` — no separate data path.

**Tech Stack:** Rust, `git2` (libgit2 bindings), `serde`/`serde_json`, `clap` (derive), `axum` + `askama` for the web UI, `similar` for diffing, `rand` for ID generation, `tempfile` + `assert_cmd` + `predicates` for tests.

## Global Constraints

- Every note is JSONL (one JSON event per line); the only mutation ever performed on note content is appending or unioning lines — never truncating or replacing wholesale. This is the single most safety-critical invariant in the codebase (see spec's Error Handling section).
- Ref layout (exact, from spec): `refs/notes/git-ticket/tickets`, `refs/notes/git-ticket/reviews`, `refs/git-ticket/tickets/<id>`, `refs/git-ticket/reviews/<id>`.
- IDs are 16 lowercase hex characters (8 random bytes), addressable by unambiguous prefix.
- Commands other than `git ticket sync` never touch the network.
- `git ticket` commands self-initialize lazily — no required `init` step before first use.
- Ticket schema is exactly: `title`, `body`, `status` (open/in-progress/closed), `assignee`. No extra fields.
- Web UI is read-only in v1 — no POST/write routes.

---

### Task 1: Workspace scaffold

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/git-ticket-core/Cargo.toml`
- Create: `crates/git-ticket-core/src/lib.rs`
- Create: `crates/git-ticket-cli/Cargo.toml`
- Create: `crates/git-ticket-cli/src/main.rs`
- Create: `.gitignore`

**Interfaces:**
- Produces: a `git-ticket` binary that runs and prints a placeholder message; a `git_ticket_core` lib crate ready for modules added in later tasks.

- [ ] **Step 1: Create the workspace root `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = ["crates/git-ticket-core", "crates/git-ticket-cli"]

[workspace.package]
version = "0.1.0"
edition = "2021"
```

- [ ] **Step 2: Create `crates/git-ticket-core/Cargo.toml`**

```toml
[package]
name = "git-ticket-core"
version.workspace = true
edition.workspace = true

[dependencies]
git2 = "0.19"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rand = "0.8"
similar = "2"
thiserror = "1"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Create `crates/git-ticket-core/src/lib.rs`**

```rust
pub mod event;
pub mod id;
pub mod log;
pub mod ticket;
pub mod review;
pub mod repo;
pub mod diff;
```

(Leave the six modules unimplemented for now — later tasks create each file. Comment out any `pub mod` lines here whose file doesn't exist yet so the workspace still builds; uncomment as each task adds its file.)

Concretely, for this task only, `lib.rs` should be:

```rust
// Modules are added incrementally by later tasks in the implementation plan.
```

- [ ] **Step 4: Create `crates/git-ticket-cli/Cargo.toml`**

```toml
[package]
name = "git-ticket-cli"
version.workspace = true
edition.workspace = true

[[bin]]
name = "git-ticket"
path = "src/main.rs"

[dependencies]
git-ticket-core = { path = "../git-ticket-core" }
clap = { version = "4", features = ["derive"] }

[dev-dependencies]
assert_cmd = "2"
predicates = "3"
tempfile = "3"
```

- [ ] **Step 5: Create `crates/git-ticket-cli/src/main.rs`**

```rust
fn main() {
    println!("git-ticket: not yet implemented");
}
```

- [ ] **Step 6: Create `.gitignore`**

```
/target
```

- [ ] **Step 7: Verify the workspace builds and runs**

Run: `cargo build && cargo run --bin git-ticket`
Expected: builds cleanly, prints `git-ticket: not yet implemented`

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml .gitignore crates
git commit -m "Scaffold git-ticket-core/git-ticket-cli workspace"
```

---

### Task 2: Event types + JSONL (de)serialization

**Files:**
- Create: `crates/git-ticket-core/src/event.rs`
- Modify: `crates/git-ticket-core/src/lib.rs` (uncomment `pub mod event;`)

**Interfaces:**
- Produces: `TicketStatus` (enum: `Open`, `InProgress`, `Closed`), `TicketEvent` (enum: `TicketCreated`, `StatusChanged`, `Assigned`, `TicketCommented`), `Verdict` (enum: `Approve`, `RequestChanges`, `Comment`), `ReviewEvent` (enum: `ReviewOpened`, `CommentAdded`, `VerdictSet`). Each event enum has `.id() -> &str`, `.ts() -> u64`, `.to_line() -> String`, `.from_line(&str) -> Option<Self>`.

- [ ] **Step 1: Write the failing tests**

```rust
// bottom of crates/git-ticket-core/src/event.rs
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-ticket-core event::`
Expected: FAIL to compile — `event` module / types don't exist yet.

- [ ] **Step 3: Write the implementation**

```rust
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
```

In `crates/git-ticket-core/src/lib.rs`, uncomment/add:

```rust
pub mod event;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p git-ticket-core event::`
Expected: PASS (4 tests)

- [ ] **Step 5: Commit**

```bash
git add crates/git-ticket-core/src/event.rs crates/git-ticket-core/src/lib.rs
git commit -m "Add TicketEvent/ReviewEvent JSONL types"
```

---

### Task 3: ID generation + prefix resolution

**Files:**
- Create: `crates/git-ticket-core/src/id.rs`
- Modify: `crates/git-ticket-core/src/lib.rs` (add `pub mod id;`)

**Interfaces:**
- Consumes: nothing from prior tasks.
- Produces: `generate_id() -> String` (16 lowercase hex chars), `resolve_prefix(prefix: &str, ids: &[String]) -> Result<String, PrefixError>`, `PrefixError` (enum: `NotFound`, `Ambiguous(Vec<String>)`).

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_id_is_16_lowercase_hex_chars() {
        let id = generate_id();
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn two_generated_ids_differ() {
        assert_ne!(generate_id(), generate_id());
    }

    #[test]
    fn resolve_prefix_finds_unique_match() {
        let ids = vec!["abc123".to_string(), "def456".to_string()];
        assert_eq!(resolve_prefix("abc", &ids), Ok("abc123".to_string()));
    }

    #[test]
    fn resolve_prefix_errors_when_not_found() {
        let ids = vec!["abc123".to_string()];
        assert_eq!(resolve_prefix("zzz", &ids), Err(PrefixError::NotFound));
    }

    #[test]
    fn resolve_prefix_errors_when_ambiguous() {
        let ids = vec!["abc123".to_string(), "abc789".to_string()];
        match resolve_prefix("abc", &ids) {
            Err(PrefixError::Ambiguous(mut matches)) => {
                matches.sort();
                assert_eq!(matches, vec!["abc123".to_string(), "abc789".to_string()]);
            }
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn resolve_prefix_accepts_full_id_as_its_own_prefix() {
        let ids = vec!["abc123".to_string()];
        assert_eq!(resolve_prefix("abc123", &ids), Ok("abc123".to_string()));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-ticket-core id::`
Expected: FAIL to compile — module doesn't exist.

- [ ] **Step 3: Write the implementation**

```rust
use rand::RngCore;

pub fn generate_id() -> String {
    let mut bytes = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefixError {
    NotFound,
    Ambiguous(Vec<String>),
}

pub fn resolve_prefix(prefix: &str, ids: &[String]) -> Result<String, PrefixError> {
    let matches: Vec<String> = ids.iter().filter(|id| id.starts_with(prefix)).cloned().collect();
    match matches.len() {
        0 => Err(PrefixError::NotFound),
        1 => Ok(matches[0].clone()),
        _ => Err(PrefixError::Ambiguous(matches)),
    }
}
```

Add to `lib.rs`: `pub mod id;`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p git-ticket-core id::`
Expected: PASS (5 tests)

- [ ] **Step 5: Commit**

```bash
git add crates/git-ticket-core/src/id.rs crates/git-ticket-core/src/lib.rs
git commit -m "Add ID generation and prefix resolution"
```

---

### Task 4: Note content operations (append, cat_sort_uniq merge)

**Files:**
- Create: `crates/git-ticket-core/src/log.rs`
- Modify: `crates/git-ticket-core/src/lib.rs` (add `pub mod log;`)

**Interfaces:**
- Consumes: nothing (pure string operations, no dependency on event.rs types).
- Produces: `parse_lines(content: &str) -> Vec<String>`, `append_line(content: &str, line: &str) -> String`, `merge_cat_sort_uniq(local: &str, remote: &str) -> String`. This module is the codebase's single safety-critical invariant: every write to a note MUST go through `append_line` or `merge_cat_sort_uniq`, never through direct string replacement.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_lines_splits_and_skips_blank_lines() {
        let content = "line1\nline2\n\nline3\n";
        assert_eq!(parse_lines(content), vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn parse_lines_on_empty_content_is_empty() {
        assert!(parse_lines("").is_empty());
    }

    #[test]
    fn append_line_adds_to_empty_content() {
        assert_eq!(append_line("", "first"), "first\n");
    }

    #[test]
    fn append_line_adds_after_existing_lines() {
        assert_eq!(append_line("first\n", "second"), "first\nsecond\n");
    }

    #[test]
    fn append_line_never_drops_existing_lines() {
        let mut content = String::new();
        for i in 0..5 {
            content = append_line(&content, &format!("event-{i}"));
        }
        let lines = parse_lines(&content);
        assert_eq!(lines, vec!["event-0", "event-1", "event-2", "event-3", "event-4"]);
    }

    #[test]
    fn merge_cat_sort_uniq_unions_and_dedupes() {
        let local = "b\na\n";
        let remote = "c\na\n";
        let merged = merge_cat_sort_uniq(local, remote);
        assert_eq!(parse_lines(&merged), vec!["a", "b", "c"]);
    }

    #[test]
    fn merge_cat_sort_uniq_with_empty_remote_keeps_local() {
        let merged = merge_cat_sort_uniq("only\n", "");
        assert_eq!(parse_lines(&merged), vec!["only"]);
    }

    #[test]
    fn merge_cat_sort_uniq_is_commutative() {
        let a = "x\ny\n";
        let b = "y\nz\n";
        assert_eq!(merge_cat_sort_uniq(a, b), merge_cat_sort_uniq(b, a));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-ticket-core log::`
Expected: FAIL to compile — module doesn't exist.

- [ ] **Step 3: Write the implementation**

```rust
pub fn parse_lines(content: &str) -> Vec<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

fn join_lines(lines: Vec<String>) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        lines.join("\n") + "\n"
    }
}

/// The ONLY sanctioned way to add a new event to a note's content.
/// Never replace or truncate existing content directly.
pub fn append_line(content: &str, line: &str) -> String {
    let mut lines = parse_lines(content);
    lines.push(line.to_string());
    join_lines(lines)
}

/// Equivalent to git's `cat_sort_uniq` notes-merge strategy, reimplemented
/// natively so sync doesn't depend on shelling out to the `git` binary.
pub fn merge_cat_sort_uniq(local: &str, remote: &str) -> String {
    let mut lines = parse_lines(local);
    lines.extend(parse_lines(remote));
    lines.sort();
    lines.dedup();
    join_lines(lines)
}
```

Add to `lib.rs`: `pub mod log;`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p git-ticket-core log::`
Expected: PASS (8 tests)

- [ ] **Step 5: Commit**

```bash
git add crates/git-ticket-core/src/log.rs crates/git-ticket-core/src/lib.rs
git commit -m "Add append-only note content operations"
```

---

### Task 5: Ticket projection (replay events into state)

**Files:**
- Modify: `crates/git-ticket-core/src/ticket.rs` (create)
- Modify: `crates/git-ticket-core/src/lib.rs` (add `pub mod ticket;`)

**Interfaces:**
- Consumes: `event::{TicketEvent, TicketStatus}` (Task 2).
- Produces: `Comment { body: String, author: String, ts: u64 }`, `TicketState { id, title, body, branch, author, created_ts, status, assignee: Option<String>, comments: Vec<Comment> }`, `project_ticket(id: &str, events: &[TicketEvent]) -> Option<TicketState>`, `project_all_tickets(events: &[TicketEvent]) -> std::collections::HashMap<String, TicketState>`.

- [ ] **Step 1: Write the failing tests**

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-ticket-core ticket::`
Expected: FAIL to compile — module doesn't exist.

- [ ] **Step 3: Write the implementation**

```rust
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
    relevant.sort_by(|a, b| a.ts().cmp(&b.ts()).then_with(|| a.to_line().cmp(&b.to_line())));

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
```

Add to `lib.rs`: `pub mod ticket;`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p git-ticket-core ticket::`
Expected: PASS (7 tests)

- [ ] **Step 5: Commit**

```bash
git add crates/git-ticket-core/src/ticket.rs crates/git-ticket-core/src/lib.rs
git commit -m "Add ticket event replay/projection"
```

---

### Task 6: Review projection (replay events into state)

**Files:**
- Create: `crates/git-ticket-core/src/review.rs`
- Modify: `crates/git-ticket-core/src/lib.rs` (add `pub mod review;`)

**Interfaces:**
- Consumes: `event::{ReviewEvent, Verdict}` (Task 2).
- Produces: `ReviewComment { thread_id: String, parent_id: Option<String>, file: String, line: u32, body: String, author: String, ts: u64 }`, `ReviewState { id, target, base, author, opened_ts, comments: Vec<ReviewComment>, verdicts: Vec<(String, Verdict, u64)> }` with `impl ReviewState { pub fn latest_verdict(&self) -> Option<&(String, Verdict, u64)> }`, `project_review(id: &str, events: &[ReviewEvent]) -> Option<ReviewState>`.

- [ ] **Step 1: Write the failing tests**

```rust
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
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-ticket-core review::`
Expected: FAIL to compile — module doesn't exist.

- [ ] **Step 3: Write the implementation**

```rust
use crate::event::{ReviewEvent, Verdict};

#[derive(Debug, Clone, PartialEq)]
pub struct ReviewComment {
    pub thread_id: String,
    pub parent_id: Option<String>,
    pub file: String,
    pub line: u32,
    pub body: String,
    pub author: String,
    pub ts: u64,
}

#[derive(Debug, Clone, PartialEq)]
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

pub fn project_review(id: &str, events: &[ReviewEvent]) -> Option<ReviewState> {
    let mut relevant: Vec<&ReviewEvent> = events.iter().filter(|e| e.id() == id).collect();
    relevant.sort_by(|a, b| a.ts().cmp(&b.ts()).then_with(|| a.to_line().cmp(&b.to_line())));

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
```

Add to `lib.rs`: `pub mod review;`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p git-ticket-core review::`
Expected: PASS (5 tests)

- [ ] **Step 5: Commit**

```bash
git add crates/git-ticket-core/src/review.rs crates/git-ticket-core/src/lib.rs
git commit -m "Add review event replay/projection"
```

---

### Task 7: Git notes read/write via git2

**Files:**
- Create: `crates/git-ticket-core/src/repo.rs`
- Modify: `crates/git-ticket-core/src/lib.rs` (add `pub mod repo;`)
- Test: `crates/git-ticket-core/tests/repo_notes.rs`

**Interfaces:**
- Consumes: `log::append_line`, `log::merge_cat_sort_uniq` (Task 4).
- Produces: `TICKETS_NOTES_REF: &str`, `REVIEWS_NOTES_REF: &str` (constants), `read_note(repo: &git2::Repository, notes_ref: &str, commit: git2::Oid) -> Option<String>`, `append_note_line(repo: &git2::Repository, notes_ref: &str, commit: git2::Oid, line: &str) -> Result<(), git2::Error>`, `ensure_merge_strategy(repo: &git2::Repository, notes_ref: &str) -> Result<(), git2::Error>` (sets `notes.<ref>.mergeStrategy = cat_sort_uniq` in repo config if unset).

- [ ] **Step 1: Write the failing integration test**

```rust
// crates/git-ticket-core/tests/repo_notes.rs
use git2::Repository;
use git_ticket_core::repo::{append_note_line, ensure_merge_strategy, read_note, TICKETS_NOTES_REF};

fn init_repo_with_one_commit() -> (tempfile::TempDir, Repository) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    let tree_id = {
        let mut index = repo.index().unwrap();
        index.write_tree().unwrap()
    };
    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[]).unwrap();
    (dir, repo)
}

#[test]
fn read_note_on_commit_with_no_note_is_none() {
    let (_dir, repo) = init_repo_with_one_commit();
    let head = repo.head().unwrap().peel_to_commit().unwrap().id();
    assert_eq!(read_note(&repo, TICKETS_NOTES_REF, head), None);
}

#[test]
fn append_note_line_creates_and_grows_a_note() {
    let (_dir, repo) = init_repo_with_one_commit();
    let head = repo.head().unwrap().peel_to_commit().unwrap().id();

    append_note_line(&repo, TICKETS_NOTES_REF, head, r#"{"n":1}"#).unwrap();
    append_note_line(&repo, TICKETS_NOTES_REF, head, r#"{"n":2}"#).unwrap();

    let content = read_note(&repo, TICKETS_NOTES_REF, head).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines, vec![r#"{"n":1}"#, r#"{"n":2}"#]);
}

#[test]
fn ensure_merge_strategy_sets_config_once() {
    let (_dir, repo) = init_repo_with_one_commit();
    ensure_merge_strategy(&repo, TICKETS_NOTES_REF).unwrap();
    let config = repo.config().unwrap();
    let value = config.get_string("notes.refs/notes/git-ticket/tickets.mergeStrategy").unwrap();
    assert_eq!(value, "cat_sort_uniq");
    // calling twice must not error
    ensure_merge_strategy(&repo, TICKETS_NOTES_REF).unwrap();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p git-ticket-core --test repo_notes`
Expected: FAIL to compile — `repo` module doesn't exist.

- [ ] **Step 3: Write the implementation**

```rust
use git2::{Error, Oid, Repository};

pub const TICKETS_NOTES_REF: &str = "refs/notes/git-ticket/tickets";
pub const REVIEWS_NOTES_REF: &str = "refs/notes/git-ticket/reviews";

pub fn read_note(repo: &Repository, notes_ref: &str, commit: Oid) -> Option<String> {
    repo.find_note(Some(notes_ref), commit)
        .ok()
        .and_then(|note| note.message().map(String::from))
}

/// Append one event line to the note on `commit`. Reads the existing note
/// (if any), grows it with `log::append_line`, and writes the result back.
/// Never replaces content wholesale.
pub fn append_note_line(repo: &Repository, notes_ref: &str, commit: Oid, line: &str) -> Result<(), Error> {
    let existing = read_note(repo, notes_ref, commit).unwrap_or_default();
    let updated = crate::log::append_line(&existing, line);
    let sig = repo.signature().or_else(|_| git2::Signature::now("git-ticket", "git-ticket@localhost"))?;
    repo.note(&sig, &sig, Some(notes_ref), commit, &updated, true)?;
    Ok(())
}

/// Merge `remote_content` into the note on `commit`, using the
/// cat_sort_uniq strategy, and write the merged result back locally.
pub fn merge_note(repo: &Repository, notes_ref: &str, commit: Oid, remote_content: &str) -> Result<(), Error> {
    let local = read_note(repo, notes_ref, commit).unwrap_or_default();
    let merged = crate::log::merge_cat_sort_uniq(&local, remote_content);
    let sig = repo.signature().or_else(|_| git2::Signature::now("git-ticket", "git-ticket@localhost"))?;
    repo.note(&sig, &sig, Some(notes_ref), commit, &merged, true)?;
    Ok(())
}

/// Lazily configure git's notes-merge strategy for `notes_ref` so that a
/// plain `git notes merge` run by a user directly also does the right
/// thing, even though `git ticket sync` itself never shells out to it.
pub fn ensure_merge_strategy(repo: &Repository, notes_ref: &str) -> Result<(), Error> {
    let mut config = repo.config()?;
    let key = format!("notes.{notes_ref}.mergeStrategy");
    if config.get_string(&key).is_err() {
        config.set_str(&key, "cat_sort_uniq")?;
    }
    Ok(())
}
```

Add to `lib.rs`: `pub mod repo;`

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p git-ticket-core --test repo_notes`
Expected: PASS (3 tests)

- [ ] **Step 5: Commit**

```bash
git add crates/git-ticket-core/src/repo.rs crates/git-ticket-core/src/lib.rs crates/git-ticket-core/tests/repo_notes.rs
git commit -m "Add git2-backed note read/append/merge"
```

---

### Task 8: Merge-base resolution + pointer refs

**Files:**
- Modify: `crates/git-ticket-core/src/repo.rs`
- Test: `crates/git-ticket-core/tests/repo_refs.rs`

**Interfaces:**
- Consumes: `git2::{Repository, Oid}`.
- Produces (added to `repo.rs`): `PointerKind` (enum: `Ticket`, `Review`), `pointer_ref_name(kind: PointerKind, id: &str) -> String`, `set_pointer_ref(repo, kind, id, target: Oid) -> Result<(), Error>`, `resolve_pointer_ref(repo, kind, id) -> Option<Oid>`, `list_pointer_ids(repo, kind) -> Vec<String>`, `merge_base(repo, a: Oid, b: Oid) -> Result<Oid, Error>`.

- [ ] **Step 1: Write the failing integration test**

```rust
// crates/git-ticket-core/tests/repo_refs.rs
use git2::Repository;
use git_ticket_core::repo::{
    list_pointer_ids, merge_base, resolve_pointer_ref, set_pointer_ref, PointerKind,
};

fn commit(repo: &Repository, msg: &str, parents: &[&git2::Commit]) -> git2::Oid {
    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    let tree_id = repo.index().unwrap().write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(None, &sig, &sig, msg, &tree, parents).unwrap()
}

#[test]
fn pointer_ref_roundtrips_and_lists() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let oid = commit(&repo, "c0", &[]);

    assert_eq!(resolve_pointer_ref(&repo, PointerKind::Ticket, "abc123"), None);
    set_pointer_ref(&repo, PointerKind::Ticket, "abc123", oid).unwrap();
    assert_eq!(resolve_pointer_ref(&repo, PointerKind::Ticket, "abc123"), Some(oid));

    let ids = list_pointer_ids(&repo, PointerKind::Ticket);
    assert_eq!(ids, vec!["abc123".to_string()]);
    assert!(list_pointer_ids(&repo, PointerKind::Review).is_empty());
}

#[test]
fn merge_base_finds_common_ancestor_of_diverged_branches() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let c0 = commit(&repo, "c0", &[]);
    let c0_commit = repo.find_commit(c0).unwrap();

    let c1 = commit(&repo, "c1 on main", &[&c0_commit]);
    let c2 = commit(&repo, "c2 on feature", &[&c0_commit]);

    assert_eq!(merge_base(&repo, c1, c2).unwrap(), c0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p git-ticket-core --test repo_refs`
Expected: FAIL to compile — these functions don't exist yet.

- [ ] **Step 3: Add to the implementation**

Append to `crates/git-ticket-core/src/repo.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerKind {
    Ticket,
    Review,
}

impl PointerKind {
    fn prefix(self) -> &'static str {
        match self {
            PointerKind::Ticket => "refs/git-ticket/tickets/",
            PointerKind::Review => "refs/git-ticket/reviews/",
        }
    }
}

pub fn pointer_ref_name(kind: PointerKind, id: &str) -> String {
    format!("{}{id}", kind.prefix())
}

pub fn set_pointer_ref(repo: &Repository, kind: PointerKind, id: &str, target: Oid) -> Result<(), Error> {
    repo.reference(&pointer_ref_name(kind, id), target, true, "git-ticket pointer")?;
    Ok(())
}

pub fn resolve_pointer_ref(repo: &Repository, kind: PointerKind, id: &str) -> Option<Oid> {
    repo.find_reference(&pointer_ref_name(kind, id))
        .ok()
        .and_then(|r| r.target())
}

pub fn list_pointer_ids(repo: &Repository, kind: PointerKind) -> Vec<String> {
    let prefix = kind.prefix();
    let glob = format!("{prefix}*");
    repo.references_glob(&glob)
        .map(|iter| {
            iter.filter_map(|r| r.ok())
                .filter_map(|r| r.name().map(|n| n.trim_start_matches(prefix).to_string()))
                .collect()
        })
        .unwrap_or_default()
}

pub fn merge_base(repo: &Repository, a: Oid, b: Oid) -> Result<Oid, Error> {
    repo.merge_base(a, b)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p git-ticket-core --test repo_refs`
Expected: PASS (2 tests)

- [ ] **Step 5: Commit**

```bash
git add crates/git-ticket-core/src/repo.rs crates/git-ticket-core/tests/repo_refs.rs
git commit -m "Add pointer ref management and merge-base resolution"
```

---

### Task 9: Ticket service (create/list/show/status/assign/comment)

**Files:**
- Create: `crates/git-ticket-core/src/ticket_service.rs`
- Modify: `crates/git-ticket-core/src/lib.rs` (add `pub mod ticket_service;`)
- Test: `crates/git-ticket-core/tests/ticket_service.rs`

**Interfaces:**
- Consumes: `repo::{TICKETS_NOTES_REF, append_note_line, ensure_merge_strategy, read_note, PointerKind, set_pointer_ref, resolve_pointer_ref, list_pointer_ids, merge_base}` (Tasks 7-8), `ticket::{TicketState, project_ticket}` (Task 5), `event::{TicketEvent, TicketStatus}` (Task 2), `id::{generate_id, resolve_prefix, PrefixError}` (Task 3).
- Produces: `TicketError` (enum: `Git(git2::Error)`, `DetachedHead`, `NotFound`, `Ambiguous(Vec<String>)`), and free functions taking `&git2::Repository`:
  - `create_ticket(repo, base_branch: &str, title: &str, body: &str, assignee: Option<&str>, author: &str, ts: u64) -> Result<TicketState, TicketError>`
  - `list_tickets(repo) -> Result<Vec<TicketState>, TicketError>`
  - `show_ticket(repo, id_prefix: &str) -> Result<TicketState, TicketError>`
  - `set_status(repo, id_prefix: &str, status: TicketStatus, ts: u64) -> Result<TicketState, TicketError>`
  - `assign_ticket(repo, id_prefix: &str, assignee: &str, ts: u64) -> Result<TicketState, TicketError>`
  - `comment_ticket(repo, id_prefix: &str, body: &str, author: &str, ts: u64) -> Result<TicketState, TicketError>`

- [ ] **Step 1: Write the failing integration test**

```rust
// crates/git-ticket-core/tests/ticket_service.rs
use git2::Repository;
use git_ticket_core::event::TicketStatus;
use git_ticket_core::ticket_service::*;

fn init_repo_with_branch(branch: &str) -> (tempfile::TempDir, Repository) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let sig = git2::Signature::now("Alex", "alex@example.com").unwrap();
    let tree_id = repo.index().unwrap().write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let oid = repo.commit(Some("HEAD"), &sig, &sig, "root commit", &tree, &[]).unwrap();
    let commit = repo.find_commit(oid).unwrap();
    repo.branch(branch, &commit, false).unwrap();
    repo.set_head(&format!("refs/heads/{branch}")).unwrap();
    (dir, repo)
}

#[test]
fn create_then_show_ticket() {
    let (_dir, repo) = init_repo_with_branch("fix/login");
    let created = create_ticket(&repo, "main", "Fix login", "details", None, "alex", 100).unwrap();
    assert_eq!(created.title, "Fix login");
    assert_eq!(created.branch, "fix/login");
    assert_eq!(created.status, TicketStatus::Open);

    let shown = show_ticket(&repo, &created.id).unwrap();
    assert_eq!(shown, created);
}

#[test]
fn show_ticket_by_unambiguous_prefix() {
    let (_dir, repo) = init_repo_with_branch("fix/login");
    let created = create_ticket(&repo, "main", "Fix login", "details", None, "alex", 100).unwrap();
    let prefix = &created.id[..4];
    let shown = show_ticket(&repo, prefix).unwrap();
    assert_eq!(shown.id, created.id);
}

#[test]
fn status_assign_and_comment_update_state() {
    let (_dir, repo) = init_repo_with_branch("fix/login");
    let created = create_ticket(&repo, "main", "Fix login", "details", None, "alex", 100).unwrap();

    set_status(&repo, &created.id, TicketStatus::InProgress, 101).unwrap();
    assign_ticket(&repo, &created.id, "bob", 102).unwrap();
    comment_ticket(&repo, &created.id, "looking into it", "bob", 103).unwrap();

    let final_state = show_ticket(&repo, &created.id).unwrap();
    assert_eq!(final_state.status, TicketStatus::InProgress);
    assert_eq!(final_state.assignee, Some("bob".to_string()));
    assert_eq!(final_state.comments.len(), 1);
    assert_eq!(final_state.comments[0].body, "looking into it");
}

#[test]
fn list_tickets_returns_all_created_tickets() {
    let (_dir, repo) = init_repo_with_branch("fix/login");
    create_ticket(&repo, "main", "First", "d1", None, "alex", 100).unwrap();
    create_ticket(&repo, "main", "Second", "d2", None, "alex", 101).unwrap();

    let all = list_tickets(&repo).unwrap();
    assert_eq!(all.len(), 2);
}

#[test]
fn show_unknown_ticket_errors_not_found() {
    let (_dir, repo) = init_repo_with_branch("fix/login");
    match show_ticket(&repo, "deadbeef") {
        Err(TicketError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p git-ticket-core --test ticket_service`
Expected: FAIL to compile — module doesn't exist.

- [ ] **Step 3: Write the implementation**

```rust
use crate::event::{TicketEvent, TicketStatus};
use crate::id::{generate_id, resolve_prefix, PrefixError};
use crate::repo::{
    append_note_line, ensure_merge_strategy, list_pointer_ids, merge_base, read_note,
    resolve_pointer_ref, set_pointer_ref, PointerKind, TICKETS_NOTES_REF,
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
    base_branch: &str,
    title: &str,
    body: &str,
    assignee: Option<&str>,
    author: &str,
    ts: u64,
) -> Result<TicketState, TicketError> {
    let head = repo.head()?;
    let branch = head.shorthand().ok_or(TicketError::DetachedHead)?.to_string();
    let tip = head.peel_to_commit()?.id();

    let root = match repo
        .find_branch(base_branch, git2::BranchType::Local)
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
```

Note: in `create_ticket`, the initial `resolve_id`/`list_pointer_ids` lookups used by `assign_ticket` won't find the brand-new id until after `set_pointer_ref` runs — this is already the order above (pointer ref is set before the optional `assign_ticket` call), so it works.

Add to `lib.rs`: `pub mod ticket_service;`

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p git-ticket-core --test ticket_service`
Expected: PASS (5 tests)

- [ ] **Step 5: Commit**

```bash
git add crates/git-ticket-core/src/ticket_service.rs crates/git-ticket-core/src/lib.rs crates/git-ticket-core/tests/ticket_service.rs
git commit -m "Add ticket service tying git plumbing to projections"
```

---

### Task 10: CLI ticket subcommands

**Files:**
- Create: `crates/git-ticket-cli/src/cli.rs`
- Create: `crates/git-ticket-cli/src/commands/mod.rs`
- Create: `crates/git-ticket-cli/src/commands/ticket.rs`
- Create: `crates/git-ticket-cli/src/git_env.rs`
- Modify: `crates/git-ticket-cli/src/main.rs`
- Test: `crates/git-ticket-cli/tests/ticket_cli.rs`

**Interfaces:**
- Consumes: `git_ticket_core::ticket_service::*` (Task 9), `git_ticket_core::event::TicketStatus` (Task 2).
- Produces: `git_env::{open_repo() -> Result<git2::Repository, String>, current_author(repo: &git2::Repository) -> String, now_ts() -> u64}`; `commands::ticket::{run_new, run_list, run_show, run_status, run_assign, run_comment}`, each taking parsed args and printing to stdout; `cli::Cli` (clap `Parser`) with a `Ticket` subcommand tree matching the spec's CLI surface for `new/list/show/status/assign/comment`.

- [ ] **Step 1: Write the failing CLI test**

```rust
// crates/git-ticket-cli/tests/ticket_cli.rs
use assert_cmd::Command;
use predicates::str::contains;
use std::process::Command as StdCommand;

fn init_repo(dir: &std::path::Path) {
    StdCommand::new("git").args(["init"]).current_dir(dir).status().unwrap();
    StdCommand::new("git").args(["config", "user.email", "test@example.com"]).current_dir(dir).status().unwrap();
    StdCommand::new("git").args(["config", "user.name", "Test User"]).current_dir(dir).status().unwrap();
    std::fs::write(dir.join("README.md"), "hello").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(dir).status().unwrap();
    StdCommand::new("git").args(["commit", "-m", "init"]).current_dir(dir).status().unwrap();
    StdCommand::new("git").args(["checkout", "-b", "fix/login"]).current_dir(dir).status().unwrap();
}

#[test]
fn new_then_list_then_show() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let mut new_cmd = Command::cargo_bin("git-ticket").unwrap();
    new_cmd.current_dir(dir.path()).args(["ticket", "new", "Fix login", "-b", "details"]);
    new_cmd.assert().success().stdout(contains("Fix login"));

    let mut list_cmd = Command::cargo_bin("git-ticket").unwrap();
    list_cmd.current_dir(dir.path()).args(["ticket", "list"]);
    list_cmd.assert().success().stdout(contains("Fix login")).stdout(contains("open"));
}

#[test]
fn status_and_assign_update_the_ticket() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let output = Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path()).args(["ticket", "new", "Fix login"])
        .output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let id = stdout.lines().next().unwrap().split_whitespace().next().unwrap();

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path()).args(["ticket", "status", id, "in-progress"])
        .assert().success();

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path()).args(["ticket", "assign", id, "bob"])
        .assert().success();

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path()).args(["ticket", "show", id])
        .assert().success().stdout(contains("in-progress")).stdout(contains("bob"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p git-ticket-cli --test ticket_cli`
Expected: FAIL — `ticket` subcommand doesn't exist yet.

- [ ] **Step 3: Write the implementation**

`crates/git-ticket-cli/src/git_env.rs`:

```rust
use git2::Repository;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn open_repo() -> Result<Repository, String> {
    Repository::discover(".").map_err(|e| format!("not a git repository: {e}"))
}

pub fn current_author(repo: &Repository) -> String {
    repo.config()
        .ok()
        .and_then(|c| c.get_string("user.name").ok())
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn now_ts() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}
```

`crates/git-ticket-cli/src/cli.rs`:

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "git-ticket")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Ticket {
        #[command(subcommand)]
        action: TicketAction,
    },
}

#[derive(Subcommand)]
pub enum TicketAction {
    New {
        title: String,
        #[arg(short = 'a', long)]
        assignee: Option<String>,
        #[arg(short = 'b', long, default_value = "")]
        body: String,
    },
    List {
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        assignee: Option<String>,
    },
    Show {
        id: String,
    },
    Status {
        id: String,
        status: String,
    },
    Assign {
        id: String,
        assignee: String,
    },
    Comment {
        id: String,
        text: String,
    },
}
```

`crates/git-ticket-cli/src/commands/mod.rs`:

```rust
pub mod ticket;
```

`crates/git-ticket-cli/src/commands/ticket.rs`:

```rust
use crate::cli::TicketAction;
use crate::git_env::{current_author, now_ts, open_repo};
use git_ticket_core::event::TicketStatus;
use git_ticket_core::ticket::TicketState;
use git_ticket_core::ticket_service::{self, TicketError};

fn status_str(status: &TicketStatus) -> &'static str {
    match status {
        TicketStatus::Open => "open",
        TicketStatus::InProgress => "in-progress",
        TicketStatus::Closed => "closed",
    }
}

fn parse_status(s: &str) -> Result<TicketStatus, String> {
    match s {
        "open" => Ok(TicketStatus::Open),
        "in-progress" => Ok(TicketStatus::InProgress),
        "closed" => Ok(TicketStatus::Closed),
        other => Err(format!("invalid status '{other}', expected open|in-progress|closed")),
    }
}

fn print_ticket_line(t: &TicketState) {
    println!(
        "{} [{}] {} (branch: {}, assignee: {})",
        t.id,
        status_str(&t.status),
        t.title,
        t.branch,
        t.assignee.as_deref().unwrap_or("-"),
    );
}

fn print_error(e: TicketError) -> ! {
    match e {
        TicketError::NotFound => eprintln!("error: ticket not found"),
        TicketError::Ambiguous(matches) => eprintln!("error: ambiguous id, matches: {}", matches.join(", ")),
        TicketError::DetachedHead => eprintln!("error: not on a branch (detached HEAD)"),
        TicketError::Git(e) => eprintln!("error: {e}"),
    }
    std::process::exit(1);
}

pub fn run(action: TicketAction) {
    let repo = match open_repo() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    let author = current_author(&repo);

    match action {
        TicketAction::New { title, assignee, body } => {
            match ticket_service::create_ticket(&repo, "main", &title, &body, assignee.as_deref(), &author, now_ts()) {
                Ok(t) => print_ticket_line(&t),
                Err(e) => print_error(e),
            }
        }
        TicketAction::List { branch, status, assignee } => {
            match ticket_service::list_tickets(&repo) {
                Ok(mut tickets) => {
                    if let Some(b) = &branch {
                        tickets.retain(|t| &t.branch == b);
                    }
                    if let Some(s) = &status {
                        if let Ok(want) = parse_status(s) {
                            tickets.retain(|t| t.status == want);
                        }
                    }
                    if let Some(a) = &assignee {
                        tickets.retain(|t| t.assignee.as_deref() == Some(a.as_str()));
                    }
                    for t in &tickets {
                        print_ticket_line(t);
                    }
                }
                Err(e) => print_error(e),
            }
        }
        TicketAction::Show { id } => match ticket_service::show_ticket(&repo, &id) {
            Ok(t) => {
                print_ticket_line(&t);
                println!("{}", t.body);
                for c in &t.comments {
                    println!("  - {} ({}): {}", c.author, c.ts, c.body);
                }
            }
            Err(e) => print_error(e),
        },
        TicketAction::Status { id, status } => {
            let status = match parse_status(&status) {
                Ok(s) => s,
                Err(msg) => {
                    eprintln!("error: {msg}");
                    std::process::exit(1);
                }
            };
            match ticket_service::set_status(&repo, &id, status, now_ts()) {
                Ok(t) => print_ticket_line(&t),
                Err(e) => print_error(e),
            }
        }
        TicketAction::Assign { id, assignee } => {
            match ticket_service::assign_ticket(&repo, &id, &assignee, now_ts()) {
                Ok(t) => print_ticket_line(&t),
                Err(e) => print_error(e),
            }
        }
        TicketAction::Comment { id, text } => {
            match ticket_service::comment_ticket(&repo, &id, &text, &author, now_ts()) {
                Ok(t) => print_ticket_line(&t),
                Err(e) => print_error(e),
            }
        }
    }
}
```

`crates/git-ticket-cli/src/main.rs`:

```rust
mod cli;
mod commands;
mod git_env;

use clap::Parser;
use cli::{Cli, Command};

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Ticket { action } => commands::ticket::run(action),
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p git-ticket-cli --test ticket_cli`
Expected: PASS (2 tests)

- [ ] **Step 5: Commit**

```bash
git add crates/git-ticket-cli
git commit -m "Add git ticket new/list/show/status/assign/comment CLI"
```

---

### Task 11: Diff computation + review service

**Files:**
- Create: `crates/git-ticket-core/src/diff.rs`
- Create: `crates/git-ticket-core/src/review_service.rs`
- Modify: `crates/git-ticket-core/src/lib.rs` (add both modules)
- Test: `crates/git-ticket-core/tests/review_service.rs`

**Interfaces:**
- Consumes: `repo::{REVIEWS_NOTES_REF, append_note_line, ensure_merge_strategy, read_note, PointerKind, set_pointer_ref, resolve_pointer_ref, list_pointer_ids, merge_base}`, `review::{ReviewState, project_review}`, `event::{ReviewEvent, Verdict}`, `id::{generate_id, resolve_prefix, PrefixError}`.
- Produces:
  - `diff.rs`: `FileDiff { path: String, hunks: Vec<DiffHunk> }`, `DiffHunk { header: String, lines: Vec<DiffLine> }`, `DiffLine { kind: DiffLineKind /* Context, Added, Removed */, old_lineno: Option<u32>, new_lineno: Option<u32>, content: String }`, `compute_diff(repo: &git2::Repository, base: git2::Oid, target: git2::Oid) -> Result<Vec<FileDiff>, git2::Error>`.
  - `review_service.rs`: `ReviewError` (same shape as `TicketError`), `start_review(repo, target: &str, base: Option<&str>, author: &str, ts: u64) -> Result<ReviewState, ReviewError>`, `add_comment(repo, id_prefix: &str, file: &str, line: u32, body: &str, reply_to: Option<&str>, author: &str, ts: u64) -> Result<ReviewState, ReviewError>`, `set_verdict(repo, id_prefix: &str, verdict: Verdict, author: &str, ts: u64) -> Result<ReviewState, ReviewError>`, `show_review(repo, id_prefix: &str) -> Result<ReviewState, ReviewError>`.

- [ ] **Step 1: Write the failing tests**

```rust
// bottom of crates/git-ticket-core/src/diff.rs
#[cfg(test)]
mod tests {
    use super::*;
    use git2::Repository;

    fn commit_file(repo: &Repository, path: &str, contents: &str, parent: Option<&git2::Commit>) -> git2::Oid {
        std::fs::write(repo.workdir().unwrap().join(path), contents).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new(path)).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();
        let parents: Vec<&git2::Commit> = parent.into_iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, "msg", &tree, &parents).unwrap()
    }

    #[test]
    fn compute_diff_reports_added_lines() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let base = commit_file(&repo, "a.txt", "line1\n", None);
        let base_commit = repo.find_commit(base).unwrap();
        let target = commit_file(&repo, "a.txt", "line1\nline2\n", Some(&base_commit));

        let diffs = compute_diff(&repo, base, target).unwrap();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "a.txt");
        let added: Vec<&str> = diffs[0].hunks.iter()
            .flat_map(|h| h.lines.iter())
            .filter(|l| matches!(l.kind, DiffLineKind::Added))
            .map(|l| l.content.as_str())
            .collect();
        assert_eq!(added, vec!["line2"]);
    }
}
```

```rust
// crates/git-ticket-core/tests/review_service.rs
use git2::Repository;
use git_ticket_core::event::Verdict;
use git_ticket_core::review_service::*;

fn init_repo_with_diverged_branch() -> (tempfile::TempDir, Repository, git2::Oid) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let sig = git2::Signature::now("Alex", "alex@example.com").unwrap();

    std::fs::write(dir.path().join("a.txt"), "line1\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let base_oid = repo.commit(Some("HEAD"), &sig, &sig, "base", &tree, &[]).unwrap();
    repo.branch("main", &repo.find_commit(base_oid).unwrap(), false).unwrap();

    repo.set_head("refs/heads/feature").ok();
    repo.branch("feature", &repo.find_commit(base_oid).unwrap(), false).unwrap();
    repo.set_head("refs/heads/feature").unwrap();

    std::fs::write(dir.path().join("a.txt"), "line1\nline2\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let base_commit = repo.find_commit(base_oid).unwrap();
    let tip = repo.commit(Some("HEAD"), &sig, &sig, "feature work", &tree, &[&base_commit]).unwrap();

    (dir, repo, tip)
}

#[test]
fn start_review_comment_and_verdict() {
    let (_dir, repo, _tip) = init_repo_with_diverged_branch();

    let review = start_review(&repo, "feature", Some("main"), "alex", 100).unwrap();
    assert_eq!(review.base, "main");

    add_comment(&repo, &review.id, "a.txt", 2, "why is this needed?", None, "bob", 101).unwrap();
    set_verdict(&repo, &review.id, Verdict::RequestChanges, "bob", 102).unwrap();

    let shown = show_review(&repo, &review.id).unwrap();
    assert_eq!(shown.comments.len(), 1);
    assert_eq!(shown.comments[0].body, "why is this needed?");
    let (author, verdict, _) = shown.latest_verdict().unwrap();
    assert_eq!(author, "bob");
    assert_eq!(*verdict, Verdict::RequestChanges);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-ticket-core diff:: --test review_service`
Expected: FAIL to compile — modules don't exist.

- [ ] **Step 3: Write the implementation**

`crates/git-ticket-core/src/diff.rs`:

```rust
use git2::{Oid, Repository};

#[derive(Debug, Clone, PartialEq)]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileDiff {
    pub path: String,
    pub hunks: Vec<DiffHunk>,
}

pub fn compute_diff(repo: &Repository, base: Oid, target: Oid) -> Result<Vec<FileDiff>, git2::Error> {
    let base_tree = repo.find_commit(base)?.tree()?;
    let target_tree = repo.find_commit(target)?.tree()?;
    let git_diff = repo.diff_tree_to_tree(Some(&base_tree), Some(&target_tree), None)?;

    let mut files: Vec<FileDiff> = Vec::new();

    git_diff.foreach(
        &mut |delta, _progress| {
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            files.push(FileDiff { path, hunks: Vec::new() });
            true
        },
        None,
        Some(&mut |_delta, hunk| {
            if let Some(file) = files.last_mut() {
                file.hunks.push(DiffHunk {
                    header: String::from_utf8_lossy(hunk.header()).trim_end().to_string(),
                    lines: Vec::new(),
                });
            }
            true
        }),
        Some(&mut |_delta, _hunk, line| {
            if let Some(file) = files.last_mut() {
                if let Some(hunk) = file.hunks.last_mut() {
                    let kind = match line.origin() {
                        '+' => DiffLineKind::Added,
                        '-' => DiffLineKind::Removed,
                        _ => DiffLineKind::Context,
                    };
                    hunk.lines.push(DiffLine {
                        kind,
                        old_lineno: line.old_lineno(),
                        new_lineno: line.new_lineno(),
                        content: String::from_utf8_lossy(line.content()).trim_end().to_string(),
                    });
                }
            }
            true
        }),
    )?;

    Ok(files)
}
```

`crates/git-ticket-core/src/review_service.rs`:

```rust
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
```

Add to `lib.rs`: `pub mod diff;` and `pub mod review_service;`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p git-ticket-core diff:: --test review_service`
Expected: PASS (2 tests)

- [ ] **Step 5: Commit**

```bash
git add crates/git-ticket-core/src/diff.rs crates/git-ticket-core/src/review_service.rs crates/git-ticket-core/src/lib.rs crates/git-ticket-core/tests/review_service.rs
git commit -m "Add diff computation and review service"
```

---

### Task 12: CLI review subcommands

**Files:**
- Modify: `crates/git-ticket-cli/src/cli.rs` (add `Review` subcommand tree)
- Create: `crates/git-ticket-cli/src/commands/review.rs`
- Modify: `crates/git-ticket-cli/src/commands/mod.rs`
- Modify: `crates/git-ticket-cli/src/main.rs`
- Test: `crates/git-ticket-cli/tests/review_cli.rs`

**Interfaces:**
- Consumes: `git_ticket_core::review_service::*` (Task 11), `git_ticket_core::event::Verdict` (Task 2), `git_env::{open_repo, current_author, now_ts}` (Task 10).
- Produces: `commands::review::run(action: ReviewAction)`; `cli::ReviewAction` (enum: `Start`, `Comment`, `Verdict`, `Show`), wired as `Command::Review { action: ReviewAction }`.

- [ ] **Step 1: Write the failing CLI test**

```rust
// crates/git-ticket-cli/tests/review_cli.rs
use assert_cmd::Command;
use predicates::str::contains;
use std::process::Command as StdCommand;

fn run(dir: &std::path::Path, args: &[&str]) {
    let status = StdCommand::new("git").args(args).current_dir(dir).status().unwrap();
    assert!(status.success());
}

fn init_repo_with_feature_branch(dir: &std::path::Path) {
    run(dir, &["init"]);
    run(dir, &["config", "user.email", "test@example.com"]);
    run(dir, &["config", "user.name", "Test User"]);
    std::fs::write(dir.join("a.txt"), "line1\n").unwrap();
    run(dir, &["add", "."]);
    run(dir, &["commit", "-m", "base"]);
    run(dir, &["branch", "-M", "main"]);
    run(dir, &["checkout", "-b", "feature"]);
    std::fs::write(dir.join("a.txt"), "line1\nline2\n").unwrap();
    run(dir, &["add", "."]);
    run(dir, &["commit", "-m", "feature work"]);
}

#[test]
fn start_comment_verdict_and_show() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_feature_branch(dir.path());

    let output = Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path())
        .args(["review", "start", "feature", "--base", "main"])
        .output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let id = stdout.lines().next().unwrap().split_whitespace().next().unwrap();

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path())
        .args(["review", "comment", id, "--file", "a.txt", "--line", "2", "why?"])
        .assert().success();

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path())
        .args(["review", "verdict", id, "approve"])
        .assert().success();

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path())
        .args(["review", "show", id])
        .assert().success()
        .stdout(contains("why?"))
        .stdout(contains("approve"))
        .stdout(contains("line2"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p git-ticket-cli --test review_cli`
Expected: FAIL — `review` subcommand doesn't exist yet.

- [ ] **Step 3: Write the implementation**

Add to `crates/git-ticket-cli/src/cli.rs` (extend `Command` and add new types):

```rust
// add a Review variant to Command:
//     Review { #[command(subcommand)] action: ReviewAction },

#[derive(Subcommand)]
pub enum ReviewAction {
    Start {
        target: Option<String>,
        #[arg(long)]
        base: Option<String>,
    },
    Comment {
        review_id: String,
        #[arg(long)]
        file: String,
        #[arg(long)]
        line: u32,
        text: String,
        #[arg(long)]
        reply_to: Option<String>,
    },
    Verdict {
        review_id: String,
        verdict: String,
        summary: Option<String>,
    },
    Show {
        review_id: String,
    },
}
```

(Full `cli.rs` after this task adds `Command::Review { action: ReviewAction }` alongside the existing `Command::Ticket` variant, and derives/imports stay the same.)

`crates/git-ticket-cli/src/commands/review.rs`:

```rust
use crate::cli::ReviewAction;
use crate::git_env::{current_author, now_ts, open_repo};
use git_ticket_core::diff::{compute_diff, DiffLineKind};
use git_ticket_core::event::Verdict;
use git_ticket_core::review::ReviewState;
use git_ticket_core::review_service::{self, ReviewError};

fn parse_verdict(s: &str) -> Result<Verdict, String> {
    match s {
        "approve" => Ok(Verdict::Approve),
        "request-changes" => Ok(Verdict::RequestChanges),
        "comment" => Ok(Verdict::Comment),
        other => Err(format!("invalid verdict '{other}', expected approve|request-changes|comment")),
    }
}

fn verdict_str(v: &Verdict) -> &'static str {
    match v {
        Verdict::Approve => "approve",
        Verdict::RequestChanges => "request-changes",
        Verdict::Comment => "comment",
    }
}

fn print_error(e: ReviewError) -> ! {
    match e {
        ReviewError::NotFound => eprintln!("error: review not found"),
        ReviewError::Ambiguous(matches) => eprintln!("error: ambiguous id, matches: {}", matches.join(", ")),
        ReviewError::InvalidTarget(t) => eprintln!("error: could not resolve '{t}'"),
        ReviewError::Git(e) => eprintln!("error: {e}"),
    }
    std::process::exit(1);
}

fn print_summary(r: &ReviewState) {
    println!("{} target={} base={}", r.id, r.target, r.base);
}

pub fn run(action: ReviewAction) {
    let repo = match open_repo() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    let author = current_author(&repo);

    match action {
        ReviewAction::Start { target, base } => {
            let target = target.unwrap_or_else(|| "HEAD".to_string());
            match review_service::start_review(&repo, &target, base.as_deref(), &author, now_ts()) {
                Ok(r) => print_summary(&r),
                Err(e) => print_error(e),
            }
        }
        ReviewAction::Comment { review_id, file, line, text, reply_to } => {
            match review_service::add_comment(&repo, &review_id, &file, line, &text, reply_to.as_deref(), &author, now_ts()) {
                Ok(r) => print_summary(&r),
                Err(e) => print_error(e),
            }
        }
        ReviewAction::Verdict { review_id, verdict, summary } => {
            let verdict = match parse_verdict(&verdict) {
                Ok(v) => v,
                Err(msg) => {
                    eprintln!("error: {msg}");
                    std::process::exit(1);
                }
            };
            match review_service::set_verdict(&repo, &review_id, verdict, &author, now_ts()) {
                Ok(r) => {
                    print_summary(&r);
                    if let Some(s) = summary {
                        println!("{s}");
                    }
                }
                Err(e) => print_error(e),
            }
        }
        ReviewAction::Show { review_id } => match review_service::show_review(&repo, &review_id) {
            Ok(r) => {
                print_summary(&r);
                if let (Ok(base_oid), Ok(target_oid)) = (
                    repo.revparse_single(&r.base).map(|o| o.id()),
                    repo.revparse_single(&r.target).map(|o| o.id()),
                ) {
                    if let Ok(files) = compute_diff(&repo, base_oid, target_oid) {
                        for f in files {
                            println!("--- {}", f.path);
                            for h in f.hunks {
                                println!("{}", h.header);
                                for l in h.lines {
                                    let marker = match l.kind {
                                        DiffLineKind::Added => "+",
                                        DiffLineKind::Removed => "-",
                                        DiffLineKind::Context => " ",
                                    };
                                    println!("{marker}{}", l.content);
                                }
                            }
                        }
                    }
                }
                for c in &r.comments {
                    println!("  [{}:{}] {} ({}): {}", c.file, c.line, c.author, c.ts, c.body);
                }
                if let Some((author, verdict, _)) = r.latest_verdict() {
                    println!("verdict: {} by {author}", verdict_str(verdict));
                }
            }
            Err(e) => print_error(e),
        },
    }
}
```

Update `crates/git-ticket-cli/src/commands/mod.rs`:

```rust
pub mod review;
pub mod ticket;
```

Update `crates/git-ticket-cli/src/main.rs`:

```rust
mod cli;
mod commands;
mod git_env;

use clap::Parser;
use cli::{Cli, Command};

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Ticket { action } => commands::ticket::run(action),
        Command::Review { action } => commands::review::run(action),
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p git-ticket-cli --test review_cli`
Expected: PASS (1 test)

- [ ] **Step 5: Commit**

```bash
git add crates/git-ticket-cli
git commit -m "Add git ticket review start/comment/verdict/show CLI"
```

---

### Task 13: Sync (fetch, cat_sort_uniq merge, push)

**Files:**
- Create: `crates/git-ticket-core/src/sync.rs`
- Modify: `crates/git-ticket-core/src/lib.rs` (add `pub mod sync;`)
- Modify: `crates/git-ticket-cli/src/cli.rs` (add `Sync` command)
- Create: `crates/git-ticket-cli/src/commands/sync.rs`
- Modify: `crates/git-ticket-cli/src/commands/mod.rs`, `main.rs`
- Test: `crates/git-ticket-core/tests/sync.rs`

**Interfaces:**
- Consumes: `repo::{TICKETS_NOTES_REF, REVIEWS_NOTES_REF, merge_note, list_pointer_ids, resolve_pointer_ref, set_pointer_ref, PointerKind}` (Tasks 7-8).
- Produces: `sync(repo: &git2::Repository, remote_name: &str) -> Result<SyncReport, git2::Error>`, `SyncReport { tickets_merged: usize, reviews_merged: usize }`. The four ref patterns from the spec are fetched from `remote_name` into a temporary local namespace, merged via `cat_sort_uniq` into the local notes, and pushed back.

**Implementation note:** rather than shelling out to `git notes merge`, sync reimplements the equivalent logic directly via `git2`, since we already have `log::merge_cat_sort_uniq` and it needs no external process — this keeps `git-ticket` a single self-contained binary with no runtime dependency on the `git` CLI being on `PATH`.

- [ ] **Step 1: Write the failing integration test**

```rust
// crates/git-ticket-core/tests/sync.rs
use git2::Repository;
use git_ticket_core::sync::sync;
use git_ticket_core::ticket_service::{create_ticket, list_tickets};

fn init_bare_remote() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    Repository::init_bare(dir.path()).unwrap();
    dir
}

fn clone_with_commit(remote_dir: &std::path::Path, branch: &str) -> (tempfile::TempDir, Repository) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::clone(remote_dir.to_str().unwrap(), dir.path()).unwrap();
    let sig = git2::Signature::now("Test", "test@example.com").unwrap();
    let tree_id = repo.index().unwrap().write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();

    // first clone into an empty bare remote has no HEAD; handle both cases
    let parents: Vec<git2::Commit> = match repo.head() {
        Ok(h) => vec![h.peel_to_commit().unwrap()],
        Err(_) => vec![],
    };
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    let oid = repo.commit(Some("HEAD"), &sig, &sig, "base", &tree, &parent_refs).unwrap();
    repo.set_head(&format!("refs/heads/{branch}")).ok();
    if repo.find_branch(branch, git2::BranchType::Local).is_err() {
        repo.branch(branch, &repo.find_commit(oid).unwrap(), false).unwrap();
        repo.set_head(&format!("refs/heads/{branch}")).unwrap();
    }
    let mut remote = repo.find_remote("origin").unwrap();
    remote.push(&[&format!("refs/heads/{branch}:refs/heads/{branch}")], None).unwrap();
    (dir, repo)
}

#[test]
fn two_clones_converge_after_sync() {
    let remote = init_bare_remote();
    let (_dir_a, repo_a) = clone_with_commit(remote.path(), "main");
    let (_dir_b, repo_b) = clone_with_commit(remote.path(), "main");

    // Both clones are now on their own "main" pointing at different commits
    // (each made its own base commit) sharing the same bare remote's refs
    // namespace but not the same branch tip — that's fine for this test,
    // which only exercises the notes/pointer-ref sync path independently
    // per clone against a shared remote.
    create_ticket(&repo_a, "main", "From A", "d", None, "alex", 100).unwrap();
    sync(&repo_a, "origin").unwrap();

    sync(&repo_b, "origin").unwrap();
    let tickets_in_b = list_tickets(&repo_b).unwrap();
    assert_eq!(tickets_in_b.len(), 1);
    assert_eq!(tickets_in_b[0].title, "From A");

    // Round-trip: B creates its own ticket, syncs, A syncs and sees both.
    create_ticket(&repo_b, "main", "From B", "d", None, "bob", 101).unwrap();
    sync(&repo_b, "origin").unwrap();
    sync(&repo_a, "origin").unwrap();
    let tickets_in_a = list_tickets(&repo_a).unwrap();
    assert_eq!(tickets_in_a.len(), 2);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p git-ticket-core --test sync`
Expected: FAIL to compile — `sync` module doesn't exist.

- [ ] **Step 3: Write the implementation**

```rust
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

fn merge_notes_ref(repo: &Repository, notes_ref: &str, fetched_notes_ref: &str) -> Result<usize, git2::Error> {
    let fetched_tip = match repo.find_reference(fetched_notes_ref) {
        Ok(r) => r.target(),
        Err(_) => return Ok(0), // remote has nothing on this ref yet
    };
    let Some(fetched_tip) = fetched_tip else { return Ok(0) };

    // The notes ref itself is a commit-like object whose tree maps
    // fanned-out note blobs to the commits they annotate. Walking every
    // commit ever noted locally or remotely and re-merging per-commit
    // keeps this simple and correct without needing to understand notes
    // tree fan-out directly: iterate blobs via git2's Notes iterator.
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
    let _ = fetched_tip;
    Ok(merged_count)
}

fn adopt_fetched_pointer_refs(repo: &Repository, kind: PointerKind, fetch_prefix: &str) -> Result<(), git2::Error> {
    let glob = format!("{fetch_prefix}*");
    let fetched: Vec<(String, Oid)> = repo
        .references_glob(&glob)?
        .filter_map(|r| r.ok())
        .filter_map(|r| {
            let name = r.name()?.to_string();
            let id = name.trim_start_matches(fetch_prefix.trim_end_matches('*')).to_string();
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
    let push_refs: Vec<&str> = push_specs.iter().map(String::as_str).collect();
    remote.push(&push_refs, None)?;

    Ok(SyncReport { tickets_merged, reviews_merged })
}
```

Add to `lib.rs`: `pub mod sync;`

Add to `crates/git-ticket-cli/src/cli.rs`: a `Command::Sync { remote: Option<String> }` variant.

`crates/git-ticket-cli/src/commands/sync.rs`:

```rust
use crate::git_env::open_repo;
use git_ticket_core::sync;

pub fn run(remote: Option<String>) {
    let repo = match open_repo() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    let remote_name = remote.unwrap_or_else(|| "origin".to_string());
    match sync::sync(&repo, &remote_name) {
        Ok(report) => println!(
            "synced: {} ticket note(s) merged, {} review note(s) merged",
            report.tickets_merged, report.reviews_merged
        ),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
```

Update `commands/mod.rs` to add `pub mod sync;` and `main.rs` to route `Command::Sync { remote } => commands::sync::run(remote)`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p git-ticket-core --test sync`
Expected: PASS (1 test)

- [ ] **Step 5: Commit**

```bash
git add crates/git-ticket-core/src/sync.rs crates/git-ticket-core/src/lib.rs crates/git-ticket-core/tests/sync.rs crates/git-ticket-cli
git commit -m "Add conflict-free sync (fetch, cat_sort_uniq merge, push)"
```

---

### Task 14: doctor command

**Files:**
- Create: `crates/git-ticket-core/src/doctor.rs`
- Modify: `crates/git-ticket-core/src/lib.rs` (add `pub mod doctor;`)
- Modify: `crates/git-ticket-cli/src/cli.rs` (add `Doctor` command)
- Create: `crates/git-ticket-cli/src/commands/doctor.rs`
- Modify: `crates/git-ticket-cli/src/commands/mod.rs`, `main.rs`
- Test: `crates/git-ticket-core/tests/doctor.rs`

**Interfaces:**
- Consumes: `repo::{list_pointer_ids, resolve_pointer_ref, read_note, PointerKind, TICKETS_NOTES_REF, REVIEWS_NOTES_REF}`.
- Produces: `Orphan { kind: PointerKind, id: String }`, `find_orphaned_pointers(repo: &git2::Repository) -> Vec<Orphan>` (pointer ref exists but its commit has no note content for that id), `prune_orphan(repo: &git2::Repository, orphan: &Orphan) -> Result<(), git2::Error>` (deletes just that pointer ref).

- [ ] **Step 1: Write the failing integration test**

```rust
// crates/git-ticket-core/tests/doctor.rs
use git2::Repository;
use git_ticket_core::doctor::{find_orphaned_pointers, prune_orphan};
use git_ticket_core::repo::{resolve_pointer_ref, set_pointer_ref, PointerKind};
use git_ticket_core::ticket_service::create_ticket;

fn init_repo_with_branch(branch: &str) -> (tempfile::TempDir, Repository) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let sig = git2::Signature::now("Alex", "alex@example.com").unwrap();
    let tree_id = repo.index().unwrap().write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let oid = repo.commit(Some("HEAD"), &sig, &sig, "root", &tree, &[]).unwrap();
    let commit = repo.find_commit(oid).unwrap();
    repo.branch(branch, &commit, false).unwrap();
    repo.set_head(&format!("refs/heads/{branch}")).unwrap();
    (dir, repo)
}

#[test]
fn finds_no_orphans_for_a_healthy_ticket() {
    let (_dir, repo) = init_repo_with_branch("fix/x");
    create_ticket(&repo, "main", "T", "d", None, "alex", 1).unwrap();
    assert!(find_orphaned_pointers(&repo).is_empty());
}

#[test]
fn finds_and_prunes_a_pointer_ref_with_no_matching_note() {
    let (_dir, repo) = init_repo_with_branch("fix/x");
    let head = repo.head().unwrap().peel_to_commit().unwrap().id();
    set_pointer_ref(&repo, PointerKind::Ticket, "orphan01", head).unwrap();

    let orphans = find_orphaned_pointers(&repo);
    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0].id, "orphan01");

    prune_orphan(&repo, &orphans[0]).unwrap();
    assert_eq!(resolve_pointer_ref(&repo, PointerKind::Ticket, "orphan01"), None);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p git-ticket-core --test doctor`
Expected: FAIL to compile — module doesn't exist.

- [ ] **Step 3: Write the implementation**

```rust
use crate::event::TicketEvent;
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

fn note_has_id(repo: &Repository, notes_ref: &str, commit: git2::Oid, id: &str) -> bool {
    read_note(repo, notes_ref, commit)
        .map(|content| content.lines().filter_map(TicketEvent::from_line).any(|e| e.id() == id))
        .unwrap_or(false)
}

pub fn find_orphaned_pointers(repo: &Repository) -> Vec<Orphan> {
    let mut orphans = Vec::new();

    for id in list_pointer_ids(repo, PointerKind::Ticket) {
        match resolve_pointer_ref(repo, PointerKind::Ticket, &id) {
            Some(commit) if note_has_id(repo, TICKETS_NOTES_REF, commit, &id) => {}
            _ => orphans.push(Orphan { kind: PointerKind::Ticket, id }),
        }
    }

    for id in list_pointer_ids(repo, PointerKind::Review) {
        match resolve_pointer_ref(repo, PointerKind::Review, &id) {
            Some(commit) if note_has_id(repo, REVIEWS_NOTES_REF, commit, &id) => {}
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
```

Note: `note_has_id` reuses `TicketEvent::from_line`, which also happens to parse the shape of `ReviewEvent`-tagged JSON incorrectly (different `type` values) — since `serde(tag = "type")` will simply fail to match and return `None` for review-shaped lines, this is safe: it just means it never matches, which is the correct behavior when checking a ticket note. Confirm this by reading `note_has_id`'s two call sites, which are always paired with the matching `notes_ref`.

Add to `lib.rs`: `pub mod doctor;`

CLI wiring (`Command::Doctor` with no args): prints each orphan and, if run with `--prune`, calls `prune_orphan`. Add a `--prune` flag to the `Doctor` command variant and implement `commands/doctor.rs` following the same `open_repo()` / print / exit pattern as Tasks 10 and 12.

`crates/git-ticket-cli/src/commands/doctor.rs`:

```rust
use crate::git_env::open_repo;
use git_ticket_core::doctor::{find_orphaned_pointers, prune_orphan};

pub fn run(prune: bool) {
    let repo = match open_repo() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    let orphans = find_orphaned_pointers(&repo);
    if orphans.is_empty() {
        println!("no issues found");
        return;
    }

    for orphan in &orphans {
        println!("orphaned pointer ref: {:?}/{}", orphan.kind, orphan.id);
        if prune {
            if let Err(e) = prune_orphan(&repo, orphan) {
                eprintln!("  failed to prune: {e}");
            } else {
                println!("  pruned");
            }
        }
    }
}
```

(`PointerKind` needs `#[derive(Debug)]`, already added in Task 8.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p git-ticket-core --test doctor`
Expected: PASS (2 tests)

- [ ] **Step 5: Commit**

```bash
git add crates/git-ticket-core/src/doctor.rs crates/git-ticket-core/src/lib.rs crates/git-ticket-core/tests/doctor.rs crates/git-ticket-cli
git commit -m "Add doctor command for orphaned pointer refs"
```

---

### Task 15: Read-only web UI

**Files:**
- Modify: `crates/git-ticket-cli/Cargo.toml` (add `axum`, `askama`, `askama_axum`, `tokio`)
- Create: `crates/git-ticket-cli/src/web/mod.rs`
- Create: `crates/git-ticket-cli/src/web/templates/tickets.html`
- Create: `crates/git-ticket-cli/src/web/templates/ticket_detail.html`
- Create: `crates/git-ticket-cli/src/web/templates/review_detail.html`
- Modify: `crates/git-ticket-cli/src/cli.rs` (add `Web { port: Option<u16> }` command)
- Create: `crates/git-ticket-cli/src/commands/web.rs`
- Modify: `crates/git-ticket-cli/src/commands/mod.rs`, `main.rs`
- Test: `crates/git-ticket-cli/tests/web.rs`

**Interfaces:**
- Consumes: `git_ticket_core::ticket_service::{list_tickets, show_ticket}`, `git_ticket_core::review_service::show_review`, `git_ticket_core::diff::compute_diff` (Tasks 9, 11).
- Produces: `web::build_router(repo_path: std::path::PathBuf) -> axum::Router`, GET routes `/`, `/tickets/:id`, `/reviews/:id`.

- [ ] **Step 1: Add dependencies**

Append to `crates/git-ticket-cli/Cargo.toml`:

```toml
axum = "0.7"
askama = "0.12"
askama_axum = "0.4"
tokio = { version = "1", features = ["rt-multi-thread", "net", "macros"] }
```

- [ ] **Step 2: Write the failing test**

```rust
// crates/git-ticket-cli/tests/web.rs
use git_ticket_cli::web::build_router;
use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt; // for `oneshot`

// Note: this test requires `git-ticket-cli` to expose a `pub mod web;`
// from a `lib.rs` in addition to its `main.rs` binary — see Step 3.

fn init_repo_with_ticket(dir: &std::path::Path) {
    let repo = git2::Repository::init(dir).unwrap();
    let sig = git2::Signature::now("Alex", "alex@example.com").unwrap();
    let tree_id = repo.index().unwrap().write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let oid = repo.commit(Some("HEAD"), &sig, &sig, "root", &tree, &[]).unwrap();
    let commit = repo.find_commit(oid).unwrap();
    repo.branch("fix/x", &commit, false).unwrap();
    repo.set_head("refs/heads/fix/x").unwrap();
    git_ticket_core::ticket_service::create_ticket(&repo, "main", "Fix it", "details", None, "alex", 1).unwrap();
}

#[tokio::test]
async fn ticket_list_page_shows_created_ticket() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_ticket(dir.path());

    let app = build_router(dir.path().to_path_buf());
    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Fix it"));
}
```

Add `tower = { version = "0.4", features = ["util"] }` to `[dev-dependencies]` in `crates/git-ticket-cli/Cargo.toml` for `.oneshot()`.

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p git-ticket-cli --test web`
Expected: FAIL to compile — `web` module isn't public yet / doesn't exist.

Since `main.rs` produces only a binary, add a `crates/git-ticket-cli/src/lib.rs` exposing `pub mod web;` so the test (and `main.rs`) can both use it:

```rust
pub mod web;
```

And change `crates/git-ticket-cli/Cargo.toml` to add a `[lib]` section:

```toml
[lib]
name = "git_ticket_cli"
path = "src/lib.rs"
```

`main.rs` keeps its existing `mod cli; mod commands; mod git_env;` but the `web` module now lives in the lib and `main.rs` calls `git_ticket_cli::web::...` from `commands/web.rs`.

- [ ] **Step 4: Write the implementation**

`crates/git-ticket-cli/src/web/mod.rs`:

```rust
use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use git_ticket_core::diff::{compute_diff, DiffLineKind};
use git_ticket_core::review_service::show_review;
use git_ticket_core::ticket::TicketState;
use git_ticket_core::ticket_service::{list_tickets, show_ticket};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    repo_path: Arc<PathBuf>,
}

fn open(state: &AppState) -> git2::Repository {
    git2::Repository::open(state.repo_path.as_path()).expect("repo path is valid")
}

#[derive(Template)]
#[template(path = "tickets.html")]
struct TicketsTemplate {
    tickets: Vec<TicketState>,
}

async fn tickets_index(State(state): State<AppState>) -> Response {
    let repo = open(&state);
    match list_tickets(&repo) {
        Ok(tickets) => TicketsTemplate { tickets }.into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "failed to list tickets").into_response(),
    }
}

#[derive(Template)]
#[template(path = "ticket_detail.html")]
struct TicketDetailTemplate {
    ticket: TicketState,
}

async fn ticket_detail(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let repo = open(&state);
    match show_ticket(&repo, &id) {
        Ok(ticket) => TicketDetailTemplate { ticket }.into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "ticket not found").into_response(),
    }
}

struct RenderedDiffLine {
    marker: &'static str,
    content: String,
}

struct RenderedFile {
    path: String,
    lines: Vec<RenderedDiffLine>,
}

#[derive(Template)]
#[template(path = "review_detail.html")]
struct ReviewDetailTemplate {
    review_id: String,
    target: String,
    base: String,
    files: Vec<RenderedFile>,
    comments: Vec<String>,
    verdict: String,
}

async fn review_detail(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let repo = open(&state);
    let review = match show_review(&repo, &id) {
        Ok(r) => r,
        Err(_) => return (StatusCode::NOT_FOUND, "review not found").into_response(),
    };

    let files = match (
        repo.revparse_single(&review.base).map(|o| o.id()),
        repo.revparse_single(&review.target).map(|o| o.id()),
    ) {
        (Ok(base), Ok(target)) => compute_diff(&repo, base, target).unwrap_or_default(),
        _ => Vec::new(),
    };

    let rendered_files = files
        .into_iter()
        .map(|f| RenderedFile {
            path: f.path,
            lines: f
                .hunks
                .into_iter()
                .flat_map(|h| h.lines)
                .map(|l| RenderedDiffLine {
                    marker: match l.kind {
                        DiffLineKind::Added => "+",
                        DiffLineKind::Removed => "-",
                        DiffLineKind::Context => " ",
                    },
                    content: l.content,
                })
                .collect(),
        })
        .collect();

    let comments = review
        .comments
        .iter()
        .map(|c| format!("{}:{} {} ({}): {}", c.file, c.line, c.author, c.ts, c.body))
        .collect();

    let verdict = review
        .latest_verdict()
        .map(|(author, v, _)| format!("{author}: {v:?}"))
        .unwrap_or_else(|| "no verdict yet".to_string());

    ReviewDetailTemplate {
        review_id: review.id,
        target: review.target,
        base: review.base,
        files: rendered_files,
        comments,
        verdict,
    }
    .into_response()
}

pub fn build_router(repo_path: PathBuf) -> Router {
    let state = AppState { repo_path: Arc::new(repo_path) };
    Router::new()
        .route("/", get(tickets_index))
        .route("/tickets/:id", get(ticket_detail))
        .route("/reviews/:id", get(review_detail))
        .with_state(state)
}
```

`crates/git-ticket-cli/src/web/templates/tickets.html`:

```html
<!doctype html>
<html>
<head><title>git-ticket</title></head>
<body>
  <h1>Tickets</h1>
  <ul>
    {% for t in tickets %}
    <li><a href="/tickets/{{ t.id }}">{{ t.title }}</a> [{{ t.status }}] ({{ t.branch }})</li>
    {% endfor %}
  </ul>
</body>
</html>
```

`crates/git-ticket-cli/src/web/templates/ticket_detail.html`:

```html
<!doctype html>
<html>
<head><title>{{ ticket.title }}</title></head>
<body>
  <h1>{{ ticket.title }}</h1>
  <p>Status: {{ ticket.status }} | Branch: {{ ticket.branch }} | Assignee: {{ ticket.assignee.as_ref().map(|a| a.as_str()).unwrap_or("-") }}</p>
  <p>{{ ticket.body }}</p>
  <h2>Comments</h2>
  <ul>
    {% for c in ticket.comments %}
    <li>{{ c.author }}: {{ c.body }}</li>
    {% endfor %}
  </ul>
</body>
</html>
```

`crates/git-ticket-cli/src/web/templates/review_detail.html`:

```html
<!doctype html>
<html>
<head><title>Review {{ review_id }}</title></head>
<body>
  <h1>Review {{ review_id }}</h1>
  <p>{{ target }} vs {{ base }} — {{ verdict }}</p>
  {% for f in files %}
  <h3>{{ f.path }}</h3>
  <pre>{% for l in f.lines %}{{ l.marker }}{{ l.content }}
{% endfor %}</pre>
  {% endfor %}
  <h2>Comments</h2>
  <ul>
    {% for c in comments %}
    <li>{{ c }}</li>
    {% endfor %}
  </ul>
</body>
</html>
```

Add to `crates/git-ticket-cli/src/cli.rs`: `Command::Web { #[arg(long)] port: Option<u16> }`.

`crates/git-ticket-cli/src/commands/web.rs`:

```rust
use crate::git_env::open_repo;

pub fn run(port: Option<u16>) {
    let repo = match open_repo() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    let repo_path = repo.path().to_path_buf();
    let port = port.unwrap_or(4747);

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async {
        let app = git_ticket_cli::web::build_router(repo_path);
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await.unwrap();
        println!("git-ticket web listening on http://127.0.0.1:{port}");
        axum::serve(listener, app).await.unwrap();
    });
}
```

Update `commands/mod.rs` to add `pub mod web;`, and `main.rs`'s match arm: `Command::Web { port } => commands::web::run(port)`.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p git-ticket-cli --test web`
Expected: PASS (1 test)

- [ ] **Step 6: Commit**

```bash
git add crates/git-ticket-cli
git commit -m "Add read-only web UI for tickets and reviews"
```

---

### Task 16: Explicit `init` command + commit trailer suggestion

**Files:**
- Modify: `crates/git-ticket-core/src/repo.rs` (small addition)
- Modify: `crates/git-ticket-cli/src/cli.rs` (add `Init` command)
- Create: `crates/git-ticket-cli/src/commands/init.rs`
- Modify: `crates/git-ticket-cli/src/commands/ticket.rs` (print trailer tip after `new`)
- Modify: `crates/git-ticket-cli/src/commands/mod.rs`, `main.rs`
- Test: `crates/git-ticket-cli/tests/init_cli.rs`

**Interfaces:**
- Consumes: `repo::{ensure_merge_strategy, TICKETS_NOTES_REF, REVIEWS_NOTES_REF}` (Task 7), `git_env::open_repo` (Task 10).
- Produces: `commands::init::run()` — explicit, idempotent setup for scripted/CI use, doing exactly what lazy self-init already does on first write (Task 9's `ensure_merge_strategy` calls). `ticket new` additionally prints a `Ticket-Id:` trailer suggestion line.

This closes two spec-listed items that earlier tasks left implicit: the spec's CLI surface names `git ticket init` explicitly (Task 9 only ever *lazily* configured the merge strategy inside `create_ticket`, with no standalone command to run it up front), and the spec's Ref & Data Layout section commits to suggesting a `Ticket-Id:` commit trailer, which no prior task ever prints.

- [ ] **Step 1: Write the failing CLI test**

```rust
// crates/git-ticket-cli/tests/init_cli.rs
use assert_cmd::Command;
use predicates::str::contains;
use std::process::Command as StdCommand;

fn init_repo(dir: &std::path::Path) {
    StdCommand::new("git").args(["init"]).current_dir(dir).status().unwrap();
    StdCommand::new("git").args(["config", "user.email", "test@example.com"]).current_dir(dir).status().unwrap();
    StdCommand::new("git").args(["config", "user.name", "Test User"]).current_dir(dir).status().unwrap();
    std::fs::write(dir.join("README.md"), "hello").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(dir).status().unwrap();
    StdCommand::new("git").args(["commit", "-m", "init"]).current_dir(dir).status().unwrap();
}

#[test]
fn init_sets_merge_strategy_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path()).arg("init")
        .assert().success().stdout(contains("configured"));

    // running again must not error
    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path()).arg("init")
        .assert().success();
}

#[test]
fn ticket_new_prints_a_trailer_suggestion() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    StdCommand::new("git").args(["checkout", "-b", "fix/login"]).current_dir(dir.path()).status().unwrap();

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path()).args(["ticket", "new", "Fix login"])
        .assert().success().stdout(contains("Ticket-Id:"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p git-ticket-cli --test init_cli`
Expected: FAIL — `init` subcommand doesn't exist, and `ticket new` doesn't print a trailer tip.

- [ ] **Step 3: Write the implementation**

Add to `crates/git-ticket-core/src/repo.rs`:

```rust
/// Runs the same idempotent setup lazily performed on first write, so
/// `git ticket init` and organic first use converge on identical state.
pub fn init_repo_config(repo: &Repository) -> Result<(), Error> {
    ensure_merge_strategy(repo, TICKETS_NOTES_REF)?;
    ensure_merge_strategy(repo, REVIEWS_NOTES_REF)?;
    Ok(())
}
```

Add to `crates/git-ticket-cli/src/cli.rs`: an `Init` variant on `Command` (no args).

`crates/git-ticket-cli/src/commands/init.rs`:

```rust
use crate::git_env::open_repo;
use git_ticket_core::repo::init_repo_config;

pub fn run() {
    let repo = match open_repo() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    match init_repo_config(&repo) {
        Ok(()) => println!("git-ticket configured (notes merge strategy set to cat_sort_uniq)"),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
```

In `crates/git-ticket-cli/src/commands/ticket.rs`, after the successful `TicketAction::New` branch prints the ticket line, add:

```rust
println!("Tip: add trailer 'Ticket-Id: {}' to commits on this branch", t.id);
```

placed directly after the existing `print_ticket_line(&t)` call inside the `TicketAction::New` match arm.

Update `commands/mod.rs` to add `pub mod init;`, and `main.rs`'s match to add `Command::Init => commands::init::run()`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p git-ticket-cli --test init_cli`
Expected: PASS (2 tests)

- [ ] **Step 5: Commit**

```bash
git add crates/git-ticket-core/src/repo.rs crates/git-ticket-cli
git commit -m "Add explicit init command and commit trailer suggestion"
```

---

## Final Verification

- [ ] Run the full test suite: `cargo test --workspace`
  Expected: all tests across both crates pass.
- [ ] Manual end-to-end walkthrough (matches spec's Verification section):
  ```bash
  cd /tmp && rm -rf gt-demo && mkdir gt-demo && cd gt-demo
  git init && git config user.email a@example.com && git config user.name A
  echo hi > README.md && git add . && git commit -m init
  git branch -M main
  git checkout -b fix/thing
  echo change >> README.md && git add . && git commit -m change
  cargo run --manifest-path /home/alex/devel/eightbits/git-ticket/Cargo.toml --bin git-ticket -- ticket new "Fix the thing"
  cargo run --manifest-path /home/alex/devel/eightbits/git-ticket/Cargo.toml --bin git-ticket -- ticket list
  cargo run --manifest-path /home/alex/devel/eightbits/git-ticket/Cargo.toml --bin git-ticket -- review start fix/thing --base main
  cargo run --manifest-path /home/alex/devel/eightbits/git-ticket/Cargo.toml --bin git-ticket -- review show <review-id>
  cargo run --manifest-path /home/alex/devel/eightbits/git-ticket/Cargo.toml --bin git-ticket -- web
  # visit http://127.0.0.1:4747 and confirm the ticket renders
  ```
- [ ] Confirm `cargo clippy --workspace` runs clean (or note any warnings for follow-up).
