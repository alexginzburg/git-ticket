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
