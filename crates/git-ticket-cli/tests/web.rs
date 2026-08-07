use git_ticket_cli::web::build_router;
use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt; // for `oneshot`

// Note: this test requires `git-ticket-cli` to expose a `pub mod web;`
// from a `lib.rs` in addition to its `main.rs` binary — see Step 3.

fn init_repo_with_ticket(dir: &std::path::Path) {
    let repo = git2::Repository::init(dir).unwrap();
    let sig = git2::Signature::now("Alex", "alex@example.com").unwrap();
    let tree_id = repo.index().unwrap().write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let oid = repo.commit(Some("HEAD"), &sig, &sig, "root", &tree, &[]).unwrap();
    let commit = repo.find_commit(oid).unwrap();
    repo.branch("fix/x", &commit, false).unwrap();
    repo.set_head("refs/heads/fix/x").unwrap();
    git_ticket_core::ticket_service::create_ticket(&repo, "main", "Fix it", "details", None, "alex", 1).unwrap();
}

#[tokio::test]
async fn ticket_list_page_shows_created_ticket() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_ticket(dir.path());

    let app = build_router(dir.path().to_path_buf());
    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Fix it"));
}
