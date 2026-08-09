use crate::cli::{OutputFormat, TicketAction, TicketTypeArg};
use crate::commands::output;
use crate::git_env::{current_author, now_ts, open_repo};
use git_ticket_core::event::{TicketStatus, TicketType};
use git_ticket_core::ticket::TicketState;
use git_ticket_core::ticket_service::{self, TicketError};
use serde::Serialize;

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

fn to_core_type(t: TicketTypeArg) -> TicketType {
    match t {
        TicketTypeArg::Task => TicketType::Task,
        TicketTypeArg::Bug => TicketType::Bug,
        TicketTypeArg::Feature => TicketType::Feature,
        TicketTypeArg::Chore => TicketType::Chore,
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

fn print_ticket_line(t: &TicketState) {
    println!(
        "{} [{}] [{}] {} (branch: {}, assignee: {})",
        t.id,
        status_str(&t.status),
        type_str(&t.ticket_type),
        t.title,
        t.branch,
        t.assignee.as_deref().unwrap_or("-"),
    );
}

fn ticket_error_message(e: &TicketError) -> String {
    match e {
        TicketError::NotFound => "ticket not found".to_string(),
        TicketError::Ambiguous(matches) => format!("ambiguous id, matches: {}", matches.join(", ")),
        TicketError::DetachedHead => "not on a branch (detached HEAD)".to_string(),
        TicketError::Git(e) => format!("{e}"),
    }
}

fn print_error(format: OutputFormat, e: TicketError) -> ! {
    output::error_exit(format, &ticket_error_message(&e))
}

/// List-view projection of `TicketState` that omits `comments`, matching
/// what the human `list` output already shows (no comment bodies) -- fetch
/// `show <id> --format json` for the full thread.
#[derive(Serialize)]
struct TicketListJson {
    id: String,
    title: String,
    body: String,
    branch: String,
    author: String,
    created_ts: u64,
    status: TicketStatus,
    ticket_type: TicketType,
    assignee: Option<String>,
}

impl From<TicketState> for TicketListJson {
    fn from(t: TicketState) -> Self {
        TicketListJson {
            id: t.id,
            title: t.title,
            body: t.body,
            branch: t.branch,
            author: t.author,
            created_ts: t.created_ts,
            status: t.status,
            ticket_type: t.ticket_type,
            assignee: t.assignee,
        }
    }
}

pub fn run(action: TicketAction, format: OutputFormat) {
    let repo = match open_repo() {
        Ok(r) => r,
        Err(e) => output::error_exit(format, &e),
    };
    let author = current_author(&repo);

    match action {
        TicketAction::New { title, assignee, body, ticket_type } => {
            // `None` => resolved by git-ticket-core from `ticket.baseBranch`
            // config, falling back to "main" -- the same policy review
            // creation uses.
            match ticket_service::create_ticket(
                &repo,
                None,
                &title,
                &body,
                assignee.as_deref(),
                to_core_type(ticket_type),
                &author,
                now_ts(),
            ) {
                Ok(t) => {
                    print_ticket_line(&t);
                    println!("Tip: add trailer 'Ticket-Id: {}' to commits on this branch", t.id);
                }
                Err(e) => print_error(format, e),
            }
        }
        TicketAction::List { branch, status, assignee, ticket_type } => {
            match ticket_service::list_tickets(&repo) {
                Ok(mut tickets) => {
                    if let Some(b) = &branch {
                        tickets.retain(|t| &t.branch == b);
                    }
                    if status != "all" {
                        match parse_status(&status) {
                            Ok(want) => tickets.retain(|t| t.status == want),
                            Err(msg) => output::error_exit(format, &msg),
                        }
                    }
                    if let Some(a) = &assignee {
                        tickets.retain(|t| t.assignee.as_deref() == Some(a.as_str()));
                    }
                    if let Some(t) = ticket_type {
                        let want = to_core_type(t);
                        tickets.retain(|t| t.ticket_type == want);
                    }
                    match format {
                        OutputFormat::Json => {
                            let json_tickets: Vec<TicketListJson> =
                                tickets.into_iter().map(TicketListJson::from).collect();
                            output::print_json(&json_tickets);
                        }
                        OutputFormat::Text => {
                            for t in &tickets {
                                print_ticket_line(t);
                            }
                        }
                    }
                }
                Err(e) => print_error(format, e),
            }
        }
        TicketAction::Show { id } => match ticket_service::show_ticket(&repo, &id) {
            Ok(t) => match format {
                OutputFormat::Json => output::print_json(&t),
                OutputFormat::Text => {
                    print_ticket_line(&t);
                    println!("{}", t.body);
                    for c in &t.comments {
                        println!("  - {} ({}): {}", c.author, c.ts, c.body);
                    }
                }
            },
            Err(e) => print_error(format, e),
        },
        TicketAction::Status { id, status } => {
            let status = match parse_status(&status) {
                Ok(s) => s,
                Err(msg) => output::error_exit(format, &msg),
            };
            match ticket_service::set_status(&repo, &id, status, now_ts()) {
                Ok(t) => print_ticket_line(&t),
                Err(e) => print_error(format, e),
            }
        }
        TicketAction::Type { id, ticket_type } => {
            match ticket_service::set_type(&repo, &id, to_core_type(ticket_type), now_ts()) {
                Ok(t) => print_ticket_line(&t),
                Err(e) => print_error(format, e),
            }
        }
        TicketAction::Assign { id, assignee } => {
            match ticket_service::assign_ticket(&repo, &id, &assignee, now_ts()) {
                Ok(t) => print_ticket_line(&t),
                Err(e) => print_error(format, e),
            }
        }
        TicketAction::Comment { id, text } => {
            match ticket_service::comment_ticket(&repo, &id, &text, &author, now_ts()) {
                Ok(t) => print_ticket_line(&t),
                Err(e) => print_error(format, e),
            }
        }
    }
}
