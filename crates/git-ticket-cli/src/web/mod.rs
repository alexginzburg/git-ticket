use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use git_ticket_core::diff::{compute_diff, DiffLineKind};
use git_ticket_core::event::{TicketStatus, TicketType};
use git_ticket_core::review_service::{review_diff_range, show_review};
use git_ticket_core::ticket_service::{list_tickets, show_ticket};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    repo_path: Arc<PathBuf>,
}

fn open(state: &AppState) -> git2::Repository {
    git2::Repository::open(state.repo_path.as_path()).expect("repo path is valid")
}

fn status_str(status: &TicketStatus) -> &'static str {
    match status {
        TicketStatus::Open => "open",
        TicketStatus::InProgress => "in-progress",
        TicketStatus::Closed => "closed",
    }
}

fn type_str(ticket_type: &TicketType) -> &'static str {
    match ticket_type {
        TicketType::Task => "task",
        TicketType::Bug => "bug",
        TicketType::Feature => "feature",
        TicketType::Chore => "chore",
    }
}

fn parse_status(s: &str) -> Option<TicketStatus> {
    match s {
        "open" => Some(TicketStatus::Open),
        "in-progress" => Some(TicketStatus::InProgress),
        "closed" => Some(TicketStatus::Closed),
        _ => None,
    }
}

struct TicketRow {
    id: String,
    title: String,
    status: &'static str,
    ticket_type: &'static str,
    branch: String,
}

#[derive(Template)]
#[template(path = "tickets.html")]
struct TicketsTemplate {
    tickets: Vec<TicketRow>,
}

async fn tickets_index(State(state): State<AppState>, Query(params): Query<HashMap<String, String>>) -> Response {
    let status_filter = params.get("status").map(String::as_str).unwrap_or("open");
    let want = if status_filter == "all" {
        None
    } else {
        match parse_status(status_filter) {
            Some(s) => Some(s),
            None => return (StatusCode::BAD_REQUEST, "invalid status filter").into_response(),
        }
    };

    let repo = open(&state);
    match list_tickets(&repo) {
        Ok(mut tickets) => {
            if let Some(want) = &want {
                tickets.retain(|t| &t.status == want);
            }
            let tickets = tickets
                .into_iter()
                .map(|t| TicketRow {
                    id: t.id,
                    title: t.title,
                    status: status_str(&t.status),
                    ticket_type: type_str(&t.ticket_type),
                    branch: t.branch,
                })
                .collect();
            TicketsTemplate { tickets }.into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "failed to list tickets").into_response(),
    }
}

struct CommentRow {
    author: String,
    body: String,
}

#[derive(Template)]
#[template(path = "ticket_detail.html")]
struct TicketDetailTemplate {
    title: String,
    status: &'static str,
    ticket_type: &'static str,
    branch: String,
    assignee: String,
    body: String,
    comments: Vec<CommentRow>,
}

async fn ticket_detail(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let repo = open(&state);
    match show_ticket(&repo, &id) {
        Ok(ticket) => TicketDetailTemplate {
            title: ticket.title,
            status: status_str(&ticket.status),
            ticket_type: type_str(&ticket.ticket_type),
            branch: ticket.branch,
            assignee: ticket.assignee.unwrap_or_else(|| "-".to_string()),
            body: ticket.body,
            comments: ticket
                .comments
                .into_iter()
                .map(|c| CommentRow { author: c.author, body: c.body })
                .collect(),
        }
        .into_response(),
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

    // Shared with the CLI's `review show` so both render the same range:
    // the snapshotted target commit vs its merge-base with the base branch.
    let files = match review_diff_range(&repo, &review) {
        Ok((base, target)) => compute_diff(&repo, base, target).unwrap_or_default(),
        Err(_) => Vec::new(),
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
