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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerKind {
    Ticket,
    Review,
}

impl PointerKind {
    fn prefix(self) -> &'static str {
        match self {
            PointerKind::Ticket => "refs/git-ticket/tickets/",
            PointerKind::Review => "refs/git-ticket/reviews/",
        }
    }
}

pub fn pointer_ref_name(kind: PointerKind, id: &str) -> String {
    format!("{}{id}", kind.prefix())
}

pub fn set_pointer_ref(repo: &Repository, kind: PointerKind, id: &str, target: Oid) -> Result<(), Error> {
    repo.reference(&pointer_ref_name(kind, id), target, true, "git-ticket pointer")?;
    Ok(())
}

pub fn resolve_pointer_ref(repo: &Repository, kind: PointerKind, id: &str) -> Option<Oid> {
    repo.find_reference(&pointer_ref_name(kind, id))
        .ok()
        .and_then(|r| r.target())
}

pub fn list_pointer_ids(repo: &Repository, kind: PointerKind) -> Vec<String> {
    let prefix = kind.prefix();
    let glob = format!("{prefix}*");
    repo.references_glob(&glob)
        .map(|iter| {
            iter.filter_map(|r| r.ok())
                .filter_map(|r| r.name().map(|n| n.trim_start_matches(prefix).to_string()))
                .collect()
        })
        .unwrap_or_default()
}

pub fn merge_base(repo: &Repository, a: Oid, b: Oid) -> Result<Oid, Error> {
    repo.merge_base(a, b)
}
