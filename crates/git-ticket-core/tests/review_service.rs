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
    let tip = repo.commit(Some("HEAD"), &sig, &sig, "feature work", &tree, &[&base_commit]).unwrap();
    drop(tree);
    drop(base_commit);

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
