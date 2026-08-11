use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::process::Command as StdCommand;

fn init_repo(dir: &std::path::Path) {
    StdCommand::new("git").args(["init", "-b", "main"]).current_dir(dir).status().unwrap();
    StdCommand::new("git").args(["config", "user.email", "test@example.com"]).current_dir(dir).status().unwrap();
    StdCommand::new("git").args(["config", "user.name", "Test User"]).current_dir(dir).status().unwrap();
    std::fs::write(dir.join("README.md"), "hello").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(dir).status().unwrap();
    StdCommand::new("git").args(["commit", "-m", "init"]).current_dir(dir).status().unwrap();
}

#[test]
fn log_decorates_commit_with_ticket_and_review() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    StdCommand::new("git").args(["checkout", "-b", "fix/login"]).current_dir(dir.path()).status().unwrap();
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(dir.path()).status().unwrap();
    StdCommand::new("git").args(["commit", "-m", "feature work"]).current_dir(dir.path()).status().unwrap();

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path()).args(["new", "Fix login bug"])
        .assert().success();

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path()).args(["review", "start"])
        .assert().success();

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path()).arg("log")
        .assert().success()
        .stdout(contains("TICKET-").and(contains("Fix login bug")).and(contains("status=open")))
        .stdout(contains("REVIEW-").and(contains("verdict=pending")));
}

#[test]
fn log_with_no_tickets_or_reviews_still_prints_commits() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path()).arg("log")
        .assert().success()
        .stdout(contains("init"))
        .stdout(contains("TICKET-").not());
}
