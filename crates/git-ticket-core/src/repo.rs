use git2::{Error, Oid, Repository};

pub const TICKETS_NOTES_REF: &str = "refs/notes/git-ticket/tickets";
pub const REVIEWS_NOTES_REF: &str = "refs/notes/git-ticket/reviews";

pub fn read_note(repo: &Repository, notes_ref: &str, commit: Oid) -> Option<String> {
    repo.find_note(Some(notes_ref), commit)
        .ok()
        .and_then(|note| note.message().map(String::from))
}

/// Append one event line to the note on `commit`. Reads the existing note
/// (if any), grows it with `log::append_line`, and writes the result back.
/// Never replaces content wholesale.
pub fn append_note_line(repo: &Repository, notes_ref: &str, commit: Oid, line: &str) -> Result<(), Error> {
    let existing = read_note(repo, notes_ref, commit).unwrap_or_default();
    let updated = crate::log::append_line(&existing, line);
    let sig = repo.signature().or_else(|_| git2::Signature::now("git-ticket", "git-ticket@localhost"))?;
    repo.note(&sig, &sig, Some(notes_ref), commit, &updated, true)?;
    Ok(())
}

/// Merge `remote_content` into the note on `commit`, using the
/// cat_sort_uniq strategy, and write the merged result back locally.
pub fn merge_note(repo: &Repository, notes_ref: &str, commit: Oid, remote_content: &str) -> Result<(), Error> {
    let local = read_note(repo, notes_ref, commit).unwrap_or_default();
    let merged = crate::log::merge_cat_sort_uniq(&local, remote_content);
    let sig = repo.signature().or_else(|_| git2::Signature::now("git-ticket", "git-ticket@localhost"))?;
    repo.note(&sig, &sig, Some(notes_ref), commit, &merged, true)?;
    Ok(())
}

/// Lazily configure git's notes-merge strategy for `notes_ref` so that a
/// plain `git notes merge` run by a user directly also does the right
/// thing, even though `git ticket sync` itself never shells out to it.
pub fn ensure_merge_strategy(repo: &Repository, notes_ref: &str) -> Result<(), Error> {
    let mut config = repo.config()?;
    let key = format!("notes.{notes_ref}.mergeStrategy");
    if config.get_string(&key).is_err() {
        config.set_str(&key, "cat_sort_uniq")?;
    }
    Ok(())
}
