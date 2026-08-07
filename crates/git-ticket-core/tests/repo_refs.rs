use git2::Repository;
use git_ticket_core::repo::{
    list_pointer_ids, merge_base, resolve_pointer_ref, set_pointer_ref, PointerKind,
};

fn commit(repo: &Repository, msg: &str, parents: &[&git2::Commit]) -> git2::Oid {
    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    let tree_id = repo.index().unwrap().write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(None, &sig, &sig, msg, &tree, parents).unwrap()
}

#[test]
fn pointer_ref_roundtrips_and_lists() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let oid = commit(&repo, "c0", &[]);

    assert_eq!(resolve_pointer_ref(&repo, PointerKind::Ticket, "abc123"), None);
    set_pointer_ref(&repo, PointerKind::Ticket, "abc123", oid).unwrap();
    assert_eq!(resolve_pointer_ref(&repo, PointerKind::Ticket, "abc123"), Some(oid));

    let ids = list_pointer_ids(&repo, PointerKind::Ticket);
    assert_eq!(ids, vec!["abc123".to_string()]);
    assert!(list_pointer_ids(&repo, PointerKind::Review).is_empty());
}

#[test]
fn merge_base_finds_common_ancestor_of_diverged_branches() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let c0 = commit(&repo, "c0", &[]);
    let c0_commit = repo.find_commit(c0).unwrap();

    let c1 = commit(&repo, "c1 on main", &[&c0_commit]);
    let c2 = commit(&repo, "c2 on feature", &[&c0_commit]);

    assert_eq!(merge_base(&repo, c1, c2).unwrap(), c0);
}
