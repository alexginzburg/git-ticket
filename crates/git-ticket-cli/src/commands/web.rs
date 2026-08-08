use crate::git_env::open_repo;

const DEFAULT_PORT: u16 = 4747;

pub fn run(port: Option<u16>) {
    let repo = match open_repo() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    let repo_path = repo.path().to_path_buf();
    let repo_name = git_ticket_cli::web::repo_name(&repo_path);

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async {
        let listener = match port {
            Some(p) => tokio::net::TcpListener::bind(("127.0.0.1", p)).await.unwrap_or_else(|e| {
                eprintln!("error: could not bind port {p}: {e}");
                std::process::exit(1);
            }),
            None => bind_default_port().await,
        };
        let actual_port = listener.local_addr().expect("bound listener has a local addr").port();
        println!("git-ticket web listening on http://localhost:{actual_port}/{repo_name}");

        let app = git_ticket_cli::web::build_router(repo_path);
        axum::serve(listener, app).await.unwrap();
    });
}

/// Tries the preferred default port first; if it's already taken (e.g. by
/// another repo's `git ticket web`), falls back to an OS-assigned free port
/// instead of failing, so multiple repos' web UIs can run at once without
/// the user having to pass `--port` manually every time.
async fn bind_default_port() -> tokio::net::TcpListener {
    match tokio::net::TcpListener::bind(("127.0.0.1", DEFAULT_PORT)).await {
        Ok(listener) => listener,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap_or_else(|e| {
                eprintln!("error: could not bind any port: {e}");
                std::process::exit(1);
            })
        }
        Err(e) => {
            eprintln!("error: could not bind port {DEFAULT_PORT}: {e}");
            std::process::exit(1);
        }
    }
}
