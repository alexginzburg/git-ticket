use crate::repo::{list_pointer_ids, resolve_pointer_ref, PointerKind};
use crate::review::ReviewState;
use crate::review_service;
use crate::ticket::TicketState;
use crate::ticket_service;
use git2::{Oid, Repository};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Annotation {
    Ticket(TicketState),
    Review(ReviewState),
}

/// Builds a map from commit oid to every ticket/review anchored there, by
/// walking the (small, bounded-by-ticket/review-count) pointer refs rather
/// than needing a reverse per-commit lookup in the services. Values are the
/// current *projected* state, not raw events, so callers see e.g. a
/// ticket's latest status rather than its creation-time snapshot.
pub fn commit_annotations(repo: &Repository) -> HashMap<Oid, Vec<Annotation>> {
    let mut map: HashMap<Oid, Vec<Annotation>> = HashMap::new();

    for id in list_pointer_ids(repo, PointerKind::Ticket) {
        let Some(oid) = resolve_pointer_ref(repo, PointerKind::Ticket, &id) else { continue };
        if let Ok(state) = ticket_service::show_ticket(repo, &id) {
            map.entry(oid).or_default().push(Annotation::Ticket(state));
        }
    }

    for id in list_pointer_ids(repo, PointerKind::Review) {
        let Some(oid) = resolve_pointer_ref(repo, PointerKind::Review, &id) else { continue };
        if let Ok(state) = review_service::show_review(repo, &id) {
            map.entry(oid).or_default().push(Annotation::Review(state));
        }
    }

    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::TicketType;

    fn init_repo_with_commits(n: u32) -> (tempfile::TempDir, Repository, Vec<Oid>) {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = git2::Signature::now("Alex", "alex@example.com").unwrap();
        let mut oids = Vec::new();
        let mut parents: Vec<Oid> = Vec::new();
        for i in 0..n {
            let tree_id = repo.index().unwrap().write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            let parent_commits: Vec<_> = parents.iter().map(|p| repo.find_commit(*p).unwrap()).collect();
            let parent_refs: Vec<&git2::Commit> = parent_commits.iter().collect();
            let oid = repo.commit(Some("HEAD"), &sig, &sig, &format!("commit {i}"), &tree, &parent_refs).unwrap();
            oids.push(oid);
            parents = vec![oid];
        }
        if repo.find_branch("main", git2::BranchType::Local).is_err() {
            let commit = repo.find_commit(*oids.last().unwrap()).unwrap();
            repo.branch("main", &commit, false).unwrap();
        }
        (dir, repo, oids)
    }

    #[test]
    fn commit_with_no_ticket_or_review_has_no_annotation() {
        let (_dir, repo, oids) = init_repo_with_commits(1);
        let map = commit_annotations(&repo);
        assert!(!map.contains_key(&oids[0]));
    }

    #[test]
    fn ticket_and_review_on_different_commits_are_each_found() {
        // main: oids[0] -- oids[1]. feature branches off oids[0] with its own
        // commit, so a ticket created from feature (base main) anchors at the
        // merge-base (oids[0]) -- distinct from main's tip (oids[1]), which
        // is where the review below gets anchored.
        let (_dir, repo, oids) = init_repo_with_commits(2);
        {
            let base_commit = repo.find_commit(oids[0]).unwrap();
            repo.branch("feature", &base_commit, false).unwrap();
        }
        repo.set_head("refs/heads/feature").unwrap();
        repo.checkout_head(None).unwrap();
        let sig = git2::Signature::now("Alex", "alex@example.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let parent = repo.find_commit(oids[0]).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "feature-only commit", &tree, &[&parent]).unwrap();

        let ticket = ticket_service::create_ticket(&repo, Some("main"), "Fix login", "", None, TicketType::Task, "alex", 1).unwrap();
        let review = review_service::start_review(&repo, &oids[1].to_string(), Some("main"), "alex", 2).unwrap();

        let map = commit_annotations(&repo);

        let ticket_anchor = resolve_pointer_ref(&repo, PointerKind::Ticket, &ticket.id).unwrap();
        assert_eq!(ticket_anchor, oids[0]);
        let ticket_annotations = map.get(&ticket_anchor).expect("ticket commit annotated");
        assert!(matches!(&ticket_annotations[0], Annotation::Ticket(t) if t.id == ticket.id));

        let review_annotations = map.get(&oids[1]).expect("review commit annotated");
        assert!(matches!(&review_annotations[0], Annotation::Review(r) if r.id == review.id));
    }

    #[test]
    fn annotation_reflects_current_projected_state_not_creation_snapshot() {
        let (_dir, repo, _oids) = init_repo_with_commits(1);
        let ticket = ticket_service::create_ticket(&repo, Some("main"), "Fix login", "", None, TicketType::Task, "alex", 1).unwrap();
        ticket_service::set_status(&repo, &ticket.id, crate::event::TicketStatus::Closed, 2).unwrap();

        let map = commit_annotations(&repo);
        let anchor = resolve_pointer_ref(&repo, PointerKind::Ticket, &ticket.id).unwrap();
        let annotations = map.get(&anchor).unwrap();
        assert!(matches!(&annotations[0], Annotation::Ticket(t) if t.status == crate::event::TicketStatus::Closed));
    }
}
