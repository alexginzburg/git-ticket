use std::cell::RefCell;

use git2::{Oid, Repository};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileDiff {
    pub path: String,
    pub hunks: Vec<DiffHunk>,
}

pub fn compute_diff(repo: &Repository, base: Oid, target: Oid) -> Result<Vec<FileDiff>, git2::Error> {
    let base_tree = repo.find_commit(base)?.tree()?;
    let target_tree = repo.find_commit(target)?.tree()?;
    let git_diff = repo.diff_tree_to_tree(Some(&base_tree), Some(&target_tree), None)?;

    let files: RefCell<Vec<FileDiff>> = RefCell::new(Vec::new());

    git_diff.foreach(
        &mut |delta, _progress| {
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            files.borrow_mut().push(FileDiff { path, hunks: Vec::new() });
            true
        },
        None,
        Some(&mut |_delta, hunk| {
            if let Some(file) = files.borrow_mut().last_mut() {
                file.hunks.push(DiffHunk {
                    header: String::from_utf8_lossy(hunk.header()).trim_end().to_string(),
                    lines: Vec::new(),
                });
            }
            true
        }),
        Some(&mut |_delta, _hunk, line| {
            if let Some(file) = files.borrow_mut().last_mut() {
                if let Some(hunk) = file.hunks.last_mut() {
                    let kind = match line.origin() {
                        '+' => DiffLineKind::Added,
                        '-' => DiffLineKind::Removed,
                        _ => DiffLineKind::Context,
                    };
                    hunk.lines.push(DiffLine {
                        kind,
                        old_lineno: line.old_lineno(),
                        new_lineno: line.new_lineno(),
                        content: String::from_utf8_lossy(line.content()).trim_end().to_string(),
                    });
                }
            }
            true
        }),
    )?;

    Ok(files.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Repository;

    fn commit_file(repo: &Repository, path: &str, contents: &str, parent: Option<&git2::Commit>) -> git2::Oid {
        std::fs::write(repo.workdir().unwrap().join(path), contents).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new(path)).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();
        let parents: Vec<&git2::Commit> = parent.into_iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, "msg", &tree, &parents).unwrap()
    }

    #[test]
    fn compute_diff_reports_added_lines() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let base = commit_file(&repo, "a.txt", "line1\n", None);
        let base_commit = repo.find_commit(base).unwrap();
        let target = commit_file(&repo, "a.txt", "line1\nline2\n", Some(&base_commit));

        let diffs = compute_diff(&repo, base, target).unwrap();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "a.txt");
        let added: Vec<&str> = diffs[0].hunks.iter()
            .flat_map(|h| h.lines.iter())
            .filter(|l| matches!(l.kind, DiffLineKind::Added))
            .map(|l| l.content.as_str())
            .collect();
        assert_eq!(added, vec!["line2"]);
    }

    #[test]
    fn file_diff_serializes_to_json_with_lowercase_line_kinds() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let base = commit_file(&repo, "a.txt", "line1\n", None);
        let base_commit = repo.find_commit(base).unwrap();
        let target = commit_file(&repo, "a.txt", "line1\nline2\n", Some(&base_commit));

        let diffs = compute_diff(&repo, base, target).unwrap();
        let json = serde_json::to_string(&diffs).unwrap();
        assert!(json.contains("\"path\""));
        assert!(json.contains("\"hunks\""));
        assert!(json.contains("\"added\""));
        assert!(!json.contains("\"Added\""));
    }
}
