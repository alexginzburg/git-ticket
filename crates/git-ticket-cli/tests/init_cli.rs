use assert_cmd::Command;
use predicates::str::contains;
use std::process::Command as StdCommand;

fn init_repo(dir: &std::path::Path) {
    StdCommand::new("git").args(["init"]).current_dir(dir).status().unwrap();
    StdCommand::new("git").args(["config", "user.email", "test@example.com"]).current_dir(dir).status().unwrap();
    StdCommand::new("git").args(["config", "user.name", "Test User"]).current_dir(dir).status().unwrap();
    std::fs::write(dir.join("README.md"), "hello").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(dir).status().unwrap();
    StdCommand::new("git").args(["commit", "-m", "init"]).current_dir(dir).status().unwrap();
}

#[test]
fn init_sets_merge_strategy_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path()).arg("init")
        .assert().success().stdout(contains("configured"));

    // running again must not error
    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path()).arg("init")
        .assert().success();
}

#[test]
fn ticket_new_prints_a_trailer_suggestion() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    StdCommand::new("git").args(["checkout", "-b", "fix/login"]).current_dir(dir.path()).status().unwrap();

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path()).args(["ticket", "new", "Fix login"])
        .assert().success().stdout(contains("Ticket-Id:"));
}
