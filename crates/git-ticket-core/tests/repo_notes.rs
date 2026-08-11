use git2::Repository;
use git_ticket_core::repo::{
    append_note_line, ensure_merge_strategy, ensure_notes_display_ref, read_note, REVIEWS_NOTES_REF,
    TICKETS_NOTES_REF,
};

fn init_repo_with_one_commit() -> (tempfile::TempDir, Repository) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
    let tree_id = {
        let mut index = repo.index().unwrap();
        index.write_tree().unwrap()
    };
    {
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[]).unwrap();
    }
    (dir, repo)
}

#[test]
fn read_note_on_commit_with_no_note_is_none() {
    let (_dir, repo) = init_repo_with_one_commit();
    let head = repo.head().unwrap().peel_to_commit().unwrap().id();
    assert_eq!(read_note(&repo, TICKETS_NOTES_REF, head), None);
}

#[test]
fn append_note_line_creates_and_grows_a_note() {
    let (_dir, repo) = init_repo_with_one_commit();
    let head = repo.head().unwrap().peel_to_commit().unwrap().id();

    append_note_line(&repo, TICKETS_NOTES_REF, head, r#"{"n":1}"#).unwrap();
    append_note_line(&repo, TICKETS_NOTES_REF, head, r#"{"n":2}"#).unwrap();

    let content = read_note(&repo, TICKETS_NOTES_REF, head).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines, vec![r#"{"n":1}"#, r#"{"n":2}"#]);
}

#[test]
fn ensure_merge_strategy_sets_config_once() {
    let (_dir, repo) = init_repo_with_one_commit();
    ensure_merge_strategy(&repo, TICKETS_NOTES_REF).unwrap();
    let config = repo.config().unwrap();
    let value = config.get_string("notes.refs/notes/git-ticket/tickets.mergeStrategy").unwrap();
    assert_eq!(value, "cat_sort_uniq");
    // calling twice must not error
    ensure_merge_strategy(&repo, TICKETS_NOTES_REF).unwrap();
}

#[test]
fn ensure_notes_display_ref_adds_both_refs_without_duplicating() {
    let (_dir, repo) = init_repo_with_one_commit();
    ensure_notes_display_ref(&repo, TICKETS_NOTES_REF).unwrap();
    ensure_notes_display_ref(&repo, REVIEWS_NOTES_REF).unwrap();
    // calling again must not duplicate the entry
    ensure_notes_display_ref(&repo, TICKETS_NOTES_REF).unwrap();

    let config = repo.config().unwrap();
    let mut entries = config.multivar("notes.displayRef", None).unwrap();
    let mut values = Vec::new();
    while let Some(Ok(entry)) = entries.next() {
        values.push(entry.value().unwrap().to_string());
    }
    assert_eq!(values, vec![TICKETS_NOTES_REF.to_string(), REVIEWS_NOTES_REF.to_string()]);
}
