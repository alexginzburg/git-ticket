use crate::cli::OutputFormat;
use crate::commands::output;
use crate::commands::review::verdict_str;
use crate::commands::ticket::{status_str, type_str};
use crate::git_env::open_repo;
use git_ticket_core::history::{commit_annotations, Annotation};
use git2::Sort;

pub fn run(revspec: Option<String>) {
    let repo = match open_repo() {
        Ok(r) => r,
        Err(e) => output::error_exit(OutputFormat::Text, &e),
    };

    let annotations = commit_annotations(&repo);

    let mut revwalk = match repo.revwalk() {
        Ok(w) => w,
        Err(e) => output::error_exit(OutputFormat::Text, &format!("{e}")),
    };
    let push_result = match &revspec {
        Some(spec) => repo
            .revparse_single(spec)
            .map_err(|e| e.to_string())
            .and_then(|obj| revwalk.push(obj.id()).map_err(|e| e.to_string())),
        None => revwalk.push_head().map_err(|e| e.to_string()),
    };
    if let Err(msg) = push_result {
        output::error_exit(OutputFormat::Text, &msg);
    }
    if let Err(e) = revwalk.set_sorting(Sort::TIME) {
        output::error_exit(OutputFormat::Text, &format!("{e}"));
    }

    for oid in revwalk.flatten() {
        let Ok(commit) = repo.find_commit(oid) else { continue };
        let summary = commit.summary().unwrap_or("");
        println!("{} {}", &oid.to_string()[..7], summary);

        if let Some(items) = annotations.get(&oid) {
            for item in items {
                match item {
                    Annotation::Ticket(t) => println!(
                        "        [TICKET-{} \"{}\" status={} type={}]",
                        t.id,
                        t.title,
                        status_str(&t.status),
                        type_str(&t.ticket_type),
                    ),
                    Annotation::Review(r) => {
                        let verdict = r.latest_verdict().map(|(_, v, _)| verdict_str(v)).unwrap_or("pending");
                        println!("        [REVIEW-{} target={} base={} verdict={}]", r.id, r.target, r.base, verdict);
                    }
                }
            }
        }
    }
}
