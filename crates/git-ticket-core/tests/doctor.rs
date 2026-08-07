use git2::Repository;
use git_ticket_core::doctor::{find_orphaned_pointers, prune_orphan};
use git_ticket_core::repo::{resolve_pointer_ref, set_pointer_ref, PointerKind};
use git_ticket_core::review_service::start_review;
use git_ticket_core::ticket_service::create_ticket;

fn init_repo_with_branch(branch: &str) -> (tempfile::TempDir, Repository) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let sig = git2::Signature::now("Alex", "alex@example.com").unwrap();
    let tree_id = repo.index().unwrap().write_tree().unwrap();
    let oid = {
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "root", &tree, &[]).unwrap()
    };
    {
        let commit = repo.find_commit(oid).unwrap();
        repo.branch(branch, &commit, false).unwrap();
    }
    repo.set_head(&format!("refs/heads/{branch}")).unwrap();
    (dir, repo)
}

fn init_repo_with_diverged_branch() -> (tempfile::TempDir, Repository) {
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
    drop(tree);
    // The local/global git config's `init.defaultBranch` may already have
    // named the initial branch "main" when the repo was initialized above
    // (and git2 refuses to force-update the branch HEAD currently points
    // at), so only create it if it doesn't already exist.
    if repo.find_branch("main", git2::BranchType::Local).is_err() {
        repo.branch("main", &repo.find_commit(base_oid).unwrap(), false).unwrap();
    }

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
    repo.commit(Some("HEAD"), &sig, &sig, "feature work", &tree, &[&base_commit]).unwrap();
    drop(tree);
    drop(base_commit);

    (dir, repo)
}

#[test]
fn finds_no_orphans_for_a_healthy_ticket() {
    let (_dir, repo) = init_repo_with_branch("fix/x");
    create_ticket(&repo, "main", "T", "d", None, "alex", 1).unwrap();
    assert!(find_orphaned_pointers(&repo).is_empty());
}

#[test]
fn finds_no_orphans_for_a_healthy_review() {
    let (_dir, repo) = init_repo_with_diverged_branch();
    start_review(&repo, "feature", Some("main"), "alex", 100).unwrap();
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
