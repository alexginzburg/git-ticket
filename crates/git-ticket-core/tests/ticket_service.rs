use git2::Repository;
use git_ticket_core::event::TicketStatus;
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

#[test]
fn create_then_show_ticket() {
    let (_dir, repo) = init_repo_with_branch("fix/login");
    let created = create_ticket(&repo, "main", "Fix login", "details", None, "alex", 100).unwrap();
    assert_eq!(created.title, "Fix login");
    assert_eq!(created.branch, "fix/login");
    assert_eq!(created.status, TicketStatus::Open);

    let shown = show_ticket(&repo, &created.id).unwrap();
    assert_eq!(shown, created);
}

#[test]
fn show_ticket_by_unambiguous_prefix() {
    let (_dir, repo) = init_repo_with_branch("fix/login");
    let created = create_ticket(&repo, "main", "Fix login", "details", None, "alex", 100).unwrap();
    let prefix = &created.id[..4];
    let shown = show_ticket(&repo, prefix).unwrap();
    assert_eq!(shown.id, created.id);
}

#[test]
fn status_assign_and_comment_update_state() {
    let (_dir, repo) = init_repo_with_branch("fix/login");
    let created = create_ticket(&repo, "main", "Fix login", "details", None, "alex", 100).unwrap();

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
    create_ticket(&repo, "main", "First", "d1", None, "alex", 100).unwrap();
    create_ticket(&repo, "main", "Second", "d2", None, "alex", 101).unwrap();

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
