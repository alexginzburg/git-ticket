# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build --workspace                                   # build both crates
cargo test --workspace                                    # run everything (unit + integration)
cargo test -p git-ticket-core                              # core crate only
cargo test -p git-ticket-cli                                # CLI crate only
cargo test -p git-ticket-core --test ticket_service          # one integration test file
cargo test -p git-ticket-core --lib ticket::                 # unit tests by module path substring
cargo clippy --workspace --all-targets                     # lint
cargo install --path crates/git-ticket-cli                 # install the `git-ticket` binary (git picks it up as `git ticket ...`)
```

Integration tests build real temporary git repos via `git2`/`tempfile` (`crates/git-ticket-core/tests/*.rs`) or drive the compiled `git-ticket` binary against one via `assert_cmd` (`crates/git-ticket-cli/tests/*.rs`) — there is no mocking of git.

## Architecture

Two-crate Cargo workspace: `git-ticket-core` (all domain logic, no CLI/network concerns) and `git-ticket-cli` (`clap` CLI + an embedded read-only `axum`/`askama` web server, both calling straight into `git-ticket-core`). The full design rationale lives in `docs/superpowers/specs/2026-08-06-git-ticket-design.md`; the implementation plan (useful for the "why" behind file layout) is in `docs/superpowers/plans/2026-08-06-git-ticket-v1.md`.

**Core idea:** tickets and code reviews are stored entirely inside git notes/refs — no server, no database. State is computed by *replaying an append-only event log*, never by mutating a stored document in place.

### The event-sourcing chain (read these files in this order to understand the core)

1. **`event.rs`** — `TicketEvent`/`ReviewEvent` enums (`TicketCreated`, `StatusChanged`, `Assigned`, `TicketCommented`, `TypeChanged` / `ReviewOpened`, `CommentAdded`, `VerdictSet`), each serializable to exactly one JSON line (`to_line`/`from_line`).
2. **`log.rs`** — the single safety-critical module in the codebase. `append_line` and `merge_cat_sort_uniq` are the *only* sanctioned ways to mutate a note's raw string content — always append or union lines, never truncate/replace. `merge_cat_sort_uniq` reimplements git's `cat_sort_uniq` notes-merge strategy natively (concat + sort + dedup lines) so two clones' edits can never produce a real git merge conflict, without shelling out to the `git` CLI.
3. **`ticket.rs` / `review.rs`** — pure replay/projection: `project_ticket`/`project_review` sort an event slice by `(timestamp, creation-event-priority, to_line())` — a three-key sort, not just timestamp — and fold it into current state (`TicketState`/`ReviewState`). The middle key exists because a `TicketCreated`/`ReviewOpened` event must always apply before same-timestamp mutation events *regardless of what order the note's lines happen to be in* (post-merge, git-note lines end up in full lexicographic order, which is not chronological — see the code comments in these two files if touching the sort).
4. **`repo.rs`** — the git2 plumbing layer: reading/writing/merging note content on specific commits (routes through `log.rs`, never bypasses it), plus **pointer refs** (`refs/git-ticket/tickets/<id>`, `refs/git-ticket/reviews/<id>`) that serve double duty as an O(1) id→commit index *and* as what keeps that commit reachable after its branch is deleted (so `git gc` doesn't collect it). Also owns `resolve_base_branch` (explicit arg → `ticket.baseBranch` git config → `"main"`), shared by both services below — don't reintroduce a second base-branch policy in a CLI command.
5. **`ticket_service.rs` / `review_service.rs`** — the public API tying the above together: resolve an id by unambiguous prefix (`id.rs`), look up its pointer ref, read+project the note, or append a new event. A **ticket** is anchored to the merge-base of its branch with the base branch (stable identity even as the branch gains commits); a **review** is anchored to the exact commit it targets (snapshot semantics — reviewing again after new commits creates a *new* review id, it never mutates the old one). `review_service::review_diff_range` is the one place that resolves a review's actual diff range (snapshotted target OID + merge-base against the *current* base branch tip) — both the CLI and the web UI call this rather than each re-deriving it. `create_ticket` takes a `TicketType` (defaults to `Task` when the caller doesn't ask for another); `set_status`/`set_type` append `StatusChanged`/`TypeChanged` the same way.
6. **`sync.rs`** — the only code path that touches the network. Fetches the four ref patterns into a scratch namespace (`refs/git-ticket-fetch/*`), merges via `repo.rs`'s merge (which routes through `log.rs`), then **force-pushes** (`+refs/...`) with a bounded fetch→merge→push retry loop, because a merged local notes commit is never a git-history descendant of the fetched remote one — a non-forced push would always be rejected as non-fast-forward. This is a deliberate last-writer-wins tradeoff on the push race window, not a bug; don't "fix" it by removing the force-push. Both the fetch and the push register a `RemoteCallbacks::credentials` callback (`credentials_callback`, in this file) that tries SSH-agent, then the git credential helper, then libgit2's default — without it, any remote requiring auth fails with "authentication required but no callback set." `SyncReport` tracks `refs_pushed` alongside the merge counts so the CLI can report what actually happened on a run that pushed but had nothing to merge.
7. **`doctor.rs`** — finds pointer refs whose target commit's note doesn't actually contain a matching event (e.g. after a partial sync). Ticket notes and review notes use different event types, so orphan-detection is split into `ticket_note_has_id`/`review_note_has_id` — don't collapse them back into one function that assumes one event type.

### CLI/web crate

`cli.rs` (clap definitions) → `commands/*.rs` (one file per subcommand group, thin dispatch + printing) → `git-ticket-core` services. `git_env.rs` supplies the real system clock/author (`git config user.name`) that core functions take as plain parameters — core itself has no notion of "now" or "who," which is what keeps it unit-testable without mocking time. `git ticket list` (and the web ticket list) default to `--status open`; `--status all`/`?status=all` shows everything — an invalid CLI value exits non-zero, an invalid web query param returns `400`. The web server (`web/mod.rs` + `web/templates/*.html`, askama) is **read-only in v1** — GET routes only, calling only `list_tickets`/`show_ticket`/`show_review`/`compute_diff`/`review_diff_range`; do not add a write route without discussing the design doc's read-only decision first. All routes are nested under `/{repo_name}` (`web::repo_name`, derived from the repo dir, handling both a `.git` path and a working-dir path since both occur — the real CLI vs. tests), and `commands/web.rs` prints the resulting URL; the default port (4747) falls back to an OS-assigned free port if already taken, so multiple repos' `git ticket web` can run concurrently without manual `--port` juggling.

Ref layout is fixed and referenced by both crates — don't hardcode a ref string anywhere outside `repo.rs`'s constants:
```
refs/notes/git-ticket/tickets     refs/notes/git-ticket/reviews
refs/git-ticket/tickets/<id>      refs/git-ticket/reviews/<id>
```
