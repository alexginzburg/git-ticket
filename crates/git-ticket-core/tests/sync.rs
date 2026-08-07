use git2::Repository;
use git_ticket_core::sync::sync;
use git_ticket_core::ticket_service::{create_ticket, list_tickets};

fn init_bare_remote() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    Repository::init_bare(dir.path()).unwrap();
    dir
}

fn clone_with_commit(remote_dir: &std::path::Path, branch: &str) -> (tempfile::TempDir, Repository) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::clone(remote_dir.to_str().unwrap(), dir.path()).unwrap();
    let sig = git2::Signature::now("Test", "test@example.com").unwrap();
    let tree_id = repo.index().unwrap().write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();

    // first clone into an empty bare remote has no HEAD; handle both cases
    let parents: Vec<git2::Commit> = match repo.head() {
        Ok(h) => vec![h.peel_to_commit().unwrap()],
        Err(_) => vec![],
    };
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    let oid = repo.commit(Some("HEAD"), &sig, &sig, "base", &tree, &parent_refs).unwrap();
    drop(tree);
    drop(parents);
    repo.set_head(&format!("refs/heads/{branch}")).ok();
    if repo.find_branch(branch, git2::BranchType::Local).is_err() {
        repo.branch(branch, &repo.find_commit(oid).unwrap(), false).unwrap();
        repo.set_head(&format!("refs/heads/{branch}")).unwrap();
    }
    let mut remote = repo.find_remote("origin").unwrap();
    remote.push(&[&format!("refs/heads/{branch}:refs/heads/{branch}")], None).unwrap();
    drop(remote);
    (dir, repo)
}

#[test]
fn two_clones_converge_after_sync() {
    let remote = init_bare_remote();
    let (_dir_a, repo_a) = clone_with_commit(remote.path(), "main");
    let (_dir_b, repo_b) = clone_with_commit(remote.path(), "main");

    // Both clones are now on their own "main" pointing at different commits
    // (each made its own base commit) sharing the same bare remote's refs
    // namespace but not the same branch tip — that's fine for this test,
    // which only exercises the notes/pointer-ref sync path independently
    // per clone against a shared remote.
    create_ticket(&repo_a, Some("main"), "From A", "d", None, "alex", 100).unwrap();
    sync(&repo_a, "origin").unwrap();

    sync(&repo_b, "origin").unwrap();
    let tickets_in_b = list_tickets(&repo_b).unwrap();
    assert_eq!(tickets_in_b.len(), 1);
    assert_eq!(tickets_in_b[0].title, "From A");

    // Round-trip: B creates its own ticket, syncs, A syncs and sees both.
    create_ticket(&repo_b, Some("main"), "From B", "d", None, "bob", 101).unwrap();
    sync(&repo_b, "origin").unwrap();
    sync(&repo_a, "origin").unwrap();
    let tickets_in_a = list_tickets(&repo_a).unwrap();
    assert_eq!(tickets_in_a.len(), 2);
}
