use git2::Repository;
use git_ticket_core::doctor::{find_orphaned_pointers, prune_orphan};
use git_ticket_core::repo::{resolve_pointer_ref, set_pointer_ref, PointerKind};
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

#[test]
fn finds_no_orphans_for_a_healthy_ticket() {
    let (_dir, repo) = init_repo_with_branch("fix/x");
    create_ticket(&repo, "main", "T", "d", None, "alex", 1).unwrap();
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
