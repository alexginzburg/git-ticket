use assert_cmd::Command;
use predicates::str::contains;
use std::process::Command as StdCommand;

fn run(dir: &std::path::Path, args: &[&str]) {
    let status = StdCommand::new("git").args(args).current_dir(dir).status().unwrap();
    assert!(status.success());
}

fn init_repo_with_feature_branch(dir: &std::path::Path) {
    run(dir, &["init", "-b", "main"]);
    run(dir, &["config", "user.email", "test@example.com"]);
    run(dir, &["config", "user.name", "Test User"]);
    std::fs::write(dir.join("a.txt"), "line1\n").unwrap();
    run(dir, &["add", "."]);
    run(dir, &["commit", "-m", "base"]);
    run(dir, &["checkout", "-b", "feature"]);
    std::fs::write(dir.join("a.txt"), "line1\nline2\n").unwrap();
    run(dir, &["add", "."]);
    run(dir, &["commit", "-m", "feature work"]);
}

#[test]
fn start_comment_verdict_and_show() {
    let dir = tempfile::tempdir().unwrap();
    init_repo_with_feature_branch(dir.path());

    let output = Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path())
        .args(["review", "start", "feature", "--base", "main"])
        .output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let id = stdout.lines().next().unwrap().split_whitespace().next().unwrap();

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path())
        .args(["review", "comment", id, "--file", "a.txt", "--line", "2", "why?"])
        .assert().success();

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path())
        .args(["review", "verdict", id, "approve"])
        .assert().success();

    Command::cargo_bin("git-ticket").unwrap()
        .current_dir(dir.path())
        .args(["review", "show", id])
        .assert().success()
        .stdout(contains("why?"))
        .stdout(contains("approve"))
        .stdout(contains("line2"));
}
