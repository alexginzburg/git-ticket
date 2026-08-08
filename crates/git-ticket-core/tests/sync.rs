use git2::Repository;
use git_ticket_core::event::TicketType;
use git_ticket_core::sync::sync;
use git_ticket_core::ticket_service::{create_ticket, list_tickets};

/// A bare remote seeded with a single commit on `main`, so that every clone
/// of it shares the *same* root commit. That shared commit is what both
/// clones' tickets anchor to, which is what makes their notes genuinely
/// conflict rather than land on disjoint commits.
fn init_seeded_remote() -> (tempfile::TempDir, tempfile::TempDir) {
    let remote_dir = tempfile::tempdir().unwrap();
    Repository::init_bare(remote_dir.path()).unwrap();

    let seed_dir = tempfile::tempdir().unwrap();
    let seed = Repository::init(seed_dir.path()).unwrap();
    let sig = git2::Signature::new("Seed", "seed@example.com", &git2::Time::new(1_000_000, 0)).unwrap();
    std::fs::write(seed_dir.path().join("a.txt"), "line1\n").unwrap();
    let mut index = seed.index().unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    {
        let tree = seed.find_tree(tree_id).unwrap();
        let oid = seed.commit(Some("HEAD"), &sig, &sig, "shared root", &tree, &[]).unwrap();
        if seed.find_branch("main", git2::BranchType::Local).is_err() {
            seed.branch("main", &seed.find_commit(oid).unwrap(), false).unwrap();
        }
    }
    seed.remote("origin", remote_dir.path().to_str().unwrap()).unwrap();
    seed.find_remote("origin")
        .unwrap()
        .push(&["+refs/heads/main:refs/heads/main"], None)
        .unwrap();
    // Point the bare remote's HEAD at main so clones check it out.
    Repository::open_bare(remote_dir.path())
        .unwrap()
        .set_head("refs/heads/main")
        .unwrap();

    (remote_dir, seed_dir)
}

fn clone_of(remote_dir: &std::path::Path) -> (tempfile::TempDir, Repository) {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::clone(remote_dir.to_str().unwrap(), dir.path()).unwrap();
    (dir, repo)
}

/// Regression test for the non-fast-forward push bug: the second clone to
/// sync merges the first clone's note into its own, producing a notes commit
/// whose only parent is its *local* notes tip -- never a descendant of the
/// remote's notes ref. A non-forced push of that is rejected, so clone B's
/// ticket would never reach the remote and every clone after the first would
/// fail on its first sync.
///
/// The two tickets deliberately anchor to the same shared root commit and
/// carry different authors and timestamps (plus distinct generated ids), so
/// the two notes commits cannot coincidentally share an OID and degenerate
/// into a no-op fast-forward.
#[test]
fn concurrent_divergent_notes_converge_across_clones() {
    let (remote, _seed) = init_seeded_remote();
    let (_dir_a, repo_a) = clone_of(remote.path());
    let (_dir_b, repo_b) = clone_of(remote.path());

    let root_a = repo_a.head().unwrap().peel_to_commit().unwrap().id();
    let root_b = repo_b.head().unwrap().peel_to_commit().unwrap().id();
    assert_eq!(root_a, root_b, "both clones must anchor tickets to the same commit");

    // Both tickets land as separate TicketCreated events on the SAME commit's
    // note, with distinct authors/timestamps so the serialized events differ.
    create_ticket(&repo_a, Some("main"), "From A", "d", None, TicketType::Task, "alex", 100).unwrap();
    create_ticket(&repo_b, Some("main"), "From B", "d", None, TicketType::Task, "bob", 20_100).unwrap();

    // A publishes first; the remote's notes ref is created from nothing.
    sync(&repo_a, "origin").unwrap();

    // B now has genuinely divergent notes history: its local notes commit is
    // not a descendant of the remote's. This push is the one that used to be
    // rejected as non-fast-forward.
    sync(&repo_b, "origin").unwrap();
    let mut titles_b: Vec<String> = list_tickets(&repo_b).unwrap().into_iter().map(|t| t.title).collect();
    titles_b.sort();
    assert_eq!(titles_b, vec!["From A".to_string(), "From B".to_string()]);

    // A syncs again and must pick up B's ticket -- proving B's merged note
    // actually reached the remote rather than being silently dropped.
    sync(&repo_a, "origin").unwrap();
    let mut titles_a: Vec<String> = list_tickets(&repo_a).unwrap().into_iter().map(|t| t.title).collect();
    titles_a.sort();
    assert_eq!(titles_a, vec!["From A".to_string(), "From B".to_string()]);
}

/// Syncing twice with nothing new in between must be a no-op, not an error:
/// the second run re-merges already-merged content (idempotent per
/// `merge_cat_sort_uniq`) and force-pushes an unchanged ref.
#[test]
fn repeated_sync_is_idempotent() {
    let (remote, _seed) = init_seeded_remote();
    let (_dir_a, repo_a) = clone_of(remote.path());

    create_ticket(&repo_a, Some("main"), "From A", "d", None, TicketType::Task, "alex", 100).unwrap();
    sync(&repo_a, "origin").unwrap();
    let report = sync(&repo_a, "origin").unwrap();
    assert_eq!(report.tickets_merged, 0);
    assert_eq!(list_tickets(&repo_a).unwrap().len(), 1);
}
