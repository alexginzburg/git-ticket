use crate::git_env::open_repo;

pub fn run(port: Option<u16>) {
    let repo = match open_repo() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    let repo_path = repo.path().to_path_buf();
    let port = port.unwrap_or(4747);

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async {
        let app = git_ticket_cli::web::build_router(repo_path);
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await.unwrap();
        println!("git-ticket web listening on http://127.0.0.1:{port}");
        axum::serve(listener, app).await.unwrap();
    });
}
