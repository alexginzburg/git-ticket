use git_ticket_cli::web::{build_router, repo_name};
use git_ticket_core::event::{TicketStatus, TicketType};
use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt; // for `oneshot`

// Note: this test requires `git-ticket-cli` to expose a `pub mod web;`
// from a `lib.rs` in addition to its `main.rs` binary — see Step 3.

fn init_repo(dir: &std::path::Path) -> git2::Repository {
    let repo = git2::Repository::init(dir).unwrap();
    let sig = git2::Signature::now("Alex", "alex@example.com").unwrap();
    {
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let oid = repo.commit(Some("HEAD"), &sig, &sig, "root", &tree, &[]).unwrap();
        let commit = repo.find_commit(oid).unwrap();
        repo.branch("fix/x", &commit, false).unwrap();
    }
    repo.set_head("refs/heads/fix/x").unwrap();
    repo
}

fn init_repo_with_ticket(dir: &std::path::Path) {
    let repo = init_repo(dir);
    git_ticket_core::ticket_service::create_ticket(&repo, Some("main"), "Fix it", "details", None, TicketType::Task, "alex", 1).unwrap();
}

async fn body_string(response: axum::response::Response) -> String {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    String::from_utf8(body.to_vec()).unwrap()
}

async fn get(app: axum::Router, uri: &str) -> axum::response::Response {
    app.oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap()).await.unwrap()
}

#[tokio::test]
async fn ticket_list_page_shows_created_ticket() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_ticket(dir.path());
    let name = repo_name(dir.path());

    let app = build_router(dir.path().to_path_buf());
    let response = get(app, &format!("/{name}")).await;

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let html = body_string(response).await;
    assert!(html.contains("Fix it"));
}

#[tokio::test]
async fn ticket_list_page_links_stay_under_the_repo_prefix() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_ticket(dir.path());
    let name = repo_name(dir.path());

    let app = build_router(dir.path().to_path_buf());
    let html = body_string(get(app, &format!("/{name}")).await).await;

    assert!(html.contains(&format!("/{name}/tickets/")));
}

#[tokio::test]
async fn ticket_list_page_defaults_to_open_status_only() {
    let dir = tempfile::tempdir().unwrap();
    let repo = init_repo(dir.path());
    let name = repo_name(dir.path());
    let open = git_ticket_core::ticket_service::create_ticket(
        &repo, Some("main"), "Stays open", "d", None, TicketType::Task, "alex", 1,
    )
    .unwrap();
    let closed = git_ticket_core::ticket_service::create_ticket(
        &repo, Some("main"), "Gets closed", "d", None, TicketType::Task, "alex", 2,
    )
    .unwrap();
    git_ticket_core::ticket_service::set_status(&repo, &closed.id, TicketStatus::Closed, 3).unwrap();

    let default_html = body_string(get(build_router(dir.path().to_path_buf()), &format!("/{name}")).await).await;
    assert!(default_html.contains(&open.title));
    assert!(!default_html.contains(&closed.title));

    let all_html =
        body_string(get(build_router(dir.path().to_path_buf()), &format!("/{name}?status=all")).await).await;
    assert!(all_html.contains(&open.title));
    assert!(all_html.contains(&closed.title));

    let closed_html =
        body_string(get(build_router(dir.path().to_path_buf()), &format!("/{name}?status=closed")).await).await;
    assert!(!closed_html.contains(&open.title));
    assert!(closed_html.contains(&closed.title));

    let bad_response = get(build_router(dir.path().to_path_buf()), &format!("/{name}?status=bogus")).await;
    assert_eq!(bad_response.status(), axum::http::StatusCode::BAD_REQUEST);
}
