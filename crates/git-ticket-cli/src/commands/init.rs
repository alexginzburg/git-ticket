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
        Ok(()) => println!("git-ticket configured (notes merge strategy set to cat_sort_uniq, notes.displayRef set for git log)"),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
