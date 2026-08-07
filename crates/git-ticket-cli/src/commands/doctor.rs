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
