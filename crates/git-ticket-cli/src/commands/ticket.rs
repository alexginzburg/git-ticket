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
