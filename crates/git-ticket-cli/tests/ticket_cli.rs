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
    StdCommand::new("git").args(["checkout", "-b", "fix/login"]).current_dir(dir).status().unwrap();
}

#[test]
fn new_then_list_then_show() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let mut new_cmd = Command::cargo_bin("git-ticket").unwrap();
    new_cmd.current_dir(dir.path()).args(["new", "Fix login", "-b", "details", "--type", "bug"]);
    new_cmd.assert().success().stdout(contains("Fix login")).stdout(contains("bug"));

    let mut list_cmd = Command::cargo_bin("git-ticket").unwrap();
    list_cmd.current_dir(dir.path()).args(["list"]);
    list_cmd.assert().success().stdout(contains("Fix login")).stdout(contains("open")).stdout(contains("bug"));

    let mut list_filtered_cmd = Command::cargo_bin("git-ticket").unwrap();
    list_filtered_cmd.current_dir(dir.path()).args(["list", "--type", "bug"]);
    list_filtered_cmd.assert().success().stdout(contains("Fix login"));

    let mut list_wrong_type_cmd = Command::cargo_bin("git-ticket").unwrap();
    list_wrong_type_cmd.current_dir(dir.path()).args(["list", "--type", "chore"]);
    list_wrong_type_cmd.assert().success().stdout(contains("Fix login").not());
}

#[test]
fn status_and_assign_update_the_ticket() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let output = Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path()).args(["new", "Fix login"])
        .output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let id = stdout.lines().next().unwrap().split_whitespace().next().unwrap();

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path()).args(["status", id, "in-progress"])
        .assert().success();

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path()).args(["assign", id, "bob"])
        .assert().success();

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path()).args(["type", id, "feature"])
        .assert().success().stdout(contains("feature"));

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path()).args(["show", id])
        .assert().success().stdout(contains("in-progress")).stdout(contains("bob")).stdout(contains("feature"));
}
