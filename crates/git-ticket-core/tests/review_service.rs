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

fn commit_on_head(dir: &std::path::Path, repo: &Repository, contents: &str, msg: &str) -> git2::Oid {
    let sig = git2::Signature::now("Alex", "alex@example.com").unwrap();
    std::fs::write(dir.join("a.txt"), contents).unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    let oid = repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &[&parent]).unwrap();
    drop(tree);
    drop(parent);
    oid
}

#[test]
fn diff_range_snapshots_target_and_uses_merge_base() {
    let (dir, repo, tip) = init_repo_with_diverged_branch();

    let review = start_review(&repo, "feature", Some("main"), "alex", 100).unwrap();
    let (base_at_open, target_at_open) = review_diff_range(&repo, &review).unwrap();
    assert_eq!(target_at_open, tip);

    // Commits landing on `feature` AFTER the review was opened must not
    // silently join the review's diff.
    let later = commit_on_head(dir.path(), &repo, "line1\nline2\nline3\n", "more feature work");
    assert_ne!(later, tip);

    let shown = show_review(&repo, &review.id).unwrap();
    let (base_now, target_now) = review_diff_range(&repo, &shown).unwrap();
    assert_eq!(target_now, tip, "target must stay pinned to the commit reviewed at open time");
    assert_eq!(base_now, base_at_open);

    // The base is the merge-base with `main`, not main's tip: advancing main
    // independently must not introduce spurious deletions into the diff.
    let main_tip_before = repo.find_branch("main", git2::BranchType::Local).unwrap().get().target().unwrap();
    assert_eq!(base_now, main_tip_before);

    repo.set_head("refs/heads/main").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();
    let new_main_tip = commit_on_head(dir.path(), &repo, "line1\nunrelated\n", "unrelated main work");
    assert_ne!(new_main_tip, main_tip_before);

    let (base_after_main_moved, target_after_main_moved) = review_diff_range(&repo, &shown).unwrap();
    assert_eq!(target_after_main_moved, tip);
    assert_eq!(
        base_after_main_moved, main_tip_before,
        "base must be merge-base(target, main), not main's moving tip"
    );
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
