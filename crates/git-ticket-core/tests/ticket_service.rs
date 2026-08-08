use git2::Repository;
use git_ticket_core::event::{TicketStatus, TicketType};
use git_ticket_core::repo::{merge_base, resolve_pointer_ref, PointerKind};
use git_ticket_core::ticket_service::*;

fn init_repo_with_branch(branch: &str) -> (tempfile::TempDir, Repository) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let sig = git2::Signature::now("Alex", "alex@example.com").unwrap();
    let tree_id = repo.index().unwrap().write_tree().unwrap();
    let oid = {
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "root commit", &tree, &[]).unwrap()
    };
    {
        let commit = repo.find_commit(oid).unwrap();
        repo.branch(branch, &commit, false).unwrap();
    }
    repo.set_head(&format!("refs/heads/{branch}")).unwrap();
    (dir, repo)
}

/// Creates a repo with a `main` branch, then a `feature` branch that
/// diverges from `main` with one extra commit. HEAD ends up on `feature`.
/// Returns (tempdir, repo, main_tip, feature_tip).
fn init_repo_with_diverging_branches() -> (tempfile::TempDir, Repository, git2::Oid, git2::Oid) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let sig = git2::Signature::now("Alex", "alex@example.com").unwrap();

    let root_oid = {
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "root commit", &tree, &[]).unwrap()
    };
    // The initial commit lands on whatever branch `git init`'s default HEAD
    // points at (commonly "main" or "master" depending on global config).
    // Only create the "main" branch explicitly if it doesn't already exist.
    if repo.find_branch("main", git2::BranchType::Local).is_err() {
        let commit = repo.find_commit(root_oid).unwrap();
        repo.branch("main", &commit, false).unwrap();
    }

    // Branch `feature` off the root commit, then add a commit only on `feature`.
    {
        let commit = repo.find_commit(root_oid).unwrap();
        repo.branch("feature", &commit, false).unwrap();
    }
    repo.set_head("refs/heads/feature").unwrap();
    repo.checkout_head(None).unwrap();

    let feature_tip = {
        let parent = repo.find_commit(root_oid).unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let oid = repo
            .commit(Some("HEAD"), &sig, &sig, "feature-only commit", &tree, &[&parent])
            .unwrap();
        repo.reference("refs/heads/feature", oid, true, "advance feature").unwrap();
        oid
    };

    (dir, repo, root_oid, feature_tip)
}

#[test]
fn create_ticket_anchors_to_merge_base_of_base_branch() {
    let (_dir, repo, main_tip, feature_tip) = init_repo_with_diverging_branches();
    assert_ne!(main_tip, feature_tip, "feature must diverge from main");

    let created = create_ticket(&repo, Some("main"), "Fix login", "details", None, TicketType::Task, "alex", 100).unwrap();
    assert_eq!(created.branch, "feature");

    let expected_base = merge_base(&repo, feature_tip, main_tip).unwrap();
    // In this setup main IS the divergence point, so the merge-base equals main's tip.
    assert_eq!(expected_base, main_tip);
    assert_ne!(expected_base, feature_tip);

    let anchored = resolve_pointer_ref(&repo, PointerKind::Ticket, &created.id).unwrap();
    assert_eq!(anchored, expected_base);
    assert_ne!(anchored, feature_tip);
}

#[test]
fn create_then_show_ticket() {
    let (_dir, repo) = init_repo_with_branch("fix/login");
    let created = create_ticket(&repo, Some("main"), "Fix login", "details", None, TicketType::Task, "alex", 100).unwrap();
    assert_eq!(created.title, "Fix login");
    assert_eq!(created.branch, "fix/login");
    assert_eq!(created.status, TicketStatus::Open);
    assert_eq!(created.ticket_type, TicketType::Task);

    let shown = show_ticket(&repo, &created.id).unwrap();
    assert_eq!(shown, created);
}

#[test]
fn create_ticket_with_explicit_type_round_trips() {
    let (_dir, repo) = init_repo_with_branch("fix/login");
    let created =
        create_ticket(&repo, Some("main"), "Fix login", "details", None, TicketType::Bug, "alex", 100).unwrap();
    assert_eq!(created.ticket_type, TicketType::Bug);

    let shown = show_ticket(&repo, &created.id).unwrap();
    assert_eq!(shown.ticket_type, TicketType::Bug);
}

#[test]
fn set_type_changes_ticket_type() {
    let (_dir, repo) = init_repo_with_branch("fix/login");
    let created =
        create_ticket(&repo, Some("main"), "Fix login", "details", None, TicketType::Task, "alex", 100).unwrap();

    set_type(&repo, &created.id, TicketType::Feature, 101).unwrap();

    let final_state = show_ticket(&repo, &created.id).unwrap();
    assert_eq!(final_state.ticket_type, TicketType::Feature);
}

#[test]
fn show_ticket_by_unambiguous_prefix() {
    let (_dir, repo) = init_repo_with_branch("fix/login");
    let created = create_ticket(&repo, Some("main"), "Fix login", "details", None, TicketType::Task, "alex", 100).unwrap();
    let prefix = &created.id[..4];
    let shown = show_ticket(&repo, prefix).unwrap();
    assert_eq!(shown.id, created.id);
}

#[test]
fn status_assign_and_comment_update_state() {
    let (_dir, repo) = init_repo_with_branch("fix/login");
    let created = create_ticket(&repo, Some("main"), "Fix login", "details", None, TicketType::Task, "alex", 100).unwrap();

    set_status(&repo, &created.id, TicketStatus::InProgress, 101).unwrap();
    assign_ticket(&repo, &created.id, "bob", 102).unwrap();
    comment_ticket(&repo, &created.id, "looking into it", "bob", 103).unwrap();

    let final_state = show_ticket(&repo, &created.id).unwrap();
    assert_eq!(final_state.status, TicketStatus::InProgress);
    assert_eq!(final_state.assignee, Some("bob".to_string()));
    assert_eq!(final_state.comments.len(), 1);
    assert_eq!(final_state.comments[0].body, "looking into it");
}

#[test]
fn list_tickets_returns_all_created_tickets() {
    let (_dir, repo) = init_repo_with_branch("fix/login");
    create_ticket(&repo, Some("main"), "First", "d1", None, TicketType::Task, "alex", 100).unwrap();
    create_ticket(&repo, Some("main"), "Second", "d2", None, TicketType::Task, "alex", 101).unwrap();

    let all = list_tickets(&repo).unwrap();
    assert_eq!(all.len(), 2);
}

#[test]
fn show_unknown_ticket_errors_not_found() {
    let (_dir, repo) = init_repo_with_branch("fix/login");
    match show_ticket(&repo, "deadbeef") {
        Err(TicketError::NotFound) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}
