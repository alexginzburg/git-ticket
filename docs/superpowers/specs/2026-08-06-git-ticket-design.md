# git-ticket: Design Spec

## Context

`git-ticket` is a new, greenfield tool (this repo is currently empty — no
commits, no branches). The goal is a ticketing + code-review system that
lives entirely inside git itself: ticket bodies and code review comments are
stored as git notes/refs rather than in an external database or SaaS
service. This is deliberate — the driving requirement is that tickets and
reviews travel *with* the code (clone, push, pull, fetch), work fully
offline, and require no server. That constraint shapes every storage
decision below.

Two existing projects were reviewed as prior art and both partially
overlap but neither covers both halves of this tool:
- **git-bug**: distributed bug tracker, issues as an append-only op-log of
  git objects under `refs/bugs/<id>`, synced via normal git push/fetch.
  Source of the "event log as git objects, synced via refs" pattern used
  here.
- **git-appraise**: code review tool storing reviews/comments in git
  notes, diff-anchored comments, approve/request-changes verdicts. Source
  of the review data model used here.
- (`h5i` was checked and is unrelated — an AI agent sandboxing tool, not a
  ticketing/notes tool.)

## Core Architecture

Git notes are single blobs per commit — two people editing the same
commit's note independently produces a real, unavoidable merge conflict if
notes are treated as mutable documents. To avoid ever hitting that
conflict, ticket/review data is stored as an **event-sourced, append-only
log**:

- Each note's content is **newline-delimited JSON (JSONL)** — one line per
  immutable event (`TicketCreated`, `StatusChanged`, `CommentAdded`,
  `VerdictSet`, etc.), tagged with an ID, timestamp, and author.
- Current state (title, status, comment thread, verdict) is computed by
  **replaying events** for a given ID, sorted deterministically
  (timestamp, tie-broken by a stable event hash).
- Because entries are only ever appended, git's built-in `cat_sort_uniq`
  notes-merge strategy resolves concurrent edits automatically —
  concatenate, sort, dedupe lines. No manual conflict resolution, ever,
  as long as the "append only, never rewrite" invariant holds.

**Reachability / GC safety + indexing:** git notes attach to commits, and
branches get deleted. We maintain lightweight pointer refs that do double
duty — keep the relevant commit reachable after branch deletion (so `git
gc` doesn't collect it) *and* serve as an O(1) index from ID → commit (no
scanning needed to answer "where's ticket `a3f9c1`'s data?").

## Ref & Data Layout

```
refs/notes/git-ticket/tickets     — JSONL events, one note per root commit
refs/notes/git-ticket/reviews     — JSONL events, one note per reviewed commit
refs/git-ticket/tickets/<id>      — pointer ref: ticket ID -> root commit
refs/git-ticket/reviews/<id>      — pointer ref: review ID -> reviewed commit
```

**Ticket linkage to branch:** a ticket is attached (via its note) to the
**root/merge-base commit** of the branch it belongs to — stable across new
commits landing on the branch. The `branch` name is recorded as a field on
the `TicketCreated` event (not inferred from ref state), so if two
branches happen to share the same merge-base, their tickets simply appear
as separate event lines on the same commit's note — no collision, because
retrieval is always by ticket ID via the pointer ref, not by scanning.

**Ticket events:** `TicketCreated{id, title, body, branch, author, ts}`,
`StatusChanged{id, status, ts}`, `Assigned{id, assignee, ts}`,
`TicketCommented{id, body, author, ts}`.

**Review linkage:** a review is snapshotted to the **specific commit
being reviewed** (branch tip at review-open time, or an explicit target
commit) — semantically different from tickets: a review is about "this
state of the code," not a persistent identity over time. If the branch
gains new commits after a review, opening review again creates a **new**
review ID rather than mutating the old one, preserving history of what was
actually reviewed and keeping state derivation (= replay of one commit's
note) simple.

**Review events:** `ReviewOpened{id, target, base, author, ts}`,
`CommentAdded{id, file, line, thread_id, parent_id, body, author, ts}`,
`VerdictSet{id, verdict /* approve|request-changes|comment */, author, ts}`.

**Diff base for a branch review:** merge-base of the branch with a
configured base branch (`git config ticket.baseBranch`, defaulting to the
repo's default branch) — matches how PR review tools compute "what's
new in this branch."

**Ticket schema (v1, minimal):** `title`, freeform `body`, `status`
(open/in-progress/closed), `assignee`.

**IDs:** short random hashes (like git short SHAs), addressable by
unambiguous prefix — same UX as `git` itself.

**Sync model:** all four ref patterns above are pushed/fetched to the same
remote as code, via `git ticket sync`. This is the *only* command that
touches the network; every other command works purely against the local
repo, which is what makes offline use fully first-class rather than a
degraded mode.

**Commit trailers:** `git ticket` commands may suggest/insert a
`Ticket-Id: <id>` trailer (via `git interpret-trailers` conventions) when
committing on a branch with an open ticket — a human-visible, tool-free
link between individual commits and a ticket, in keeping with the
"everything lives with the code" philosophy. `.mailmap` support, GPG
signing of review verdicts, and git hooks integration (post-checkout /
pre-push reminders) are explicitly deferred to a later iteration — not
core to v1.

## Crate Layout

A two-crate Cargo workspace:
- `git-ticket-core` (lib): event log types, append/replay/projection
  logic, git plumbing (via `git2`), ID generation, ref/note management.
  No CLI parsing, no networking beyond what `git2` needs for fetch/push —
  keeps this crate unit-testable in isolation.
- `git-ticket` (bin): CLI subcommand parsing (`clap`), the embedded web
  server, and the `askama` templates. Depends on `git-ticket-core` for all
  actual logic.

This split keeps the "core" invariant (append-only event log, conflict-
free merge) testable without spinning up a CLI process or HTTP server for
every test, and keeps the web server as a thin read-only view over the
same core the CLI uses — no separate data path to drift out of sync.

## CLI Surface

```
git ticket init                                   # explicit: sets notes.*.mergeStrategy=cat_sort_uniq, writes repo config
git ticket new "<title>" [-a <assignee>] [-b <body>]
git ticket list [--branch <b>] [--status <s>] [--assignee <a>]
git ticket show <id>
git ticket status <id> <open|in-progress|closed>
git ticket assign <id> <user>
git ticket comment <id> "<text>"

git ticket review start [<branch>|<commit>] [--base <ref>]
git ticket review comment <review-id> --file <path> --line <n> "<text>" [--reply-to <thread-id>]
git ticket review verdict <review-id> approve|request-changes|comment ["<summary>"]
git ticket review show <review-id>                 # diff + threaded comments, in terminal

git ticket sync [<remote>]                          # fetch + notes merge (cat_sort_uniq) + push, all git-ticket refs
git ticket web [--port <n>]                          # local read-only server
git ticket doctor                                    # diagnose orphaned pointer refs / notes, see Edge Cases
```

`git ticket new` auto-detects the current branch and its root commit — no
manual ID wiring for the common case.

**Initialization:** commands **self-initialize lazily** — the first
command that needs to write a note checks whether
`notes.<ref>.mergeStrategy` is set to `cat_sort_uniq` for the relevant
refs and sets it if not, rather than requiring a separate `git ticket
init` step first. This matters for "easily used with an existing repo" —
no setup ceremony required before `git ticket new` just works. `git
ticket init` still exists for explicit/scripted setup (e.g. CI, or a repo
owner who wants config committed/documented up front) but is optional.

## Web UI (v1: read-only)

Server-rendered HTML using `askama` — no separate JS build/toolchain,
keeps the tool a single static binary, consistent with the offline-
first/single-binary story. Minimal JS only where genuinely needed (e.g.
comment-thread hover affordances). Views: ticket list (filterable),
ticket detail (event timeline), review detail (diff computed via
`git2`/`similar`, comments overlaid by file/line, verdict badge). No
write/POST routes in v1 — all creation/editing is CLI-only, so no
auth/CSRF story is needed yet. Server: `axum`.

## Data Flow Example: Opening a Review

1. `git ticket review start feature/x` → resolve `feature/x` tip commit +
   merge-base with configured base branch.
2. Generate review ID, append `ReviewOpened` event to the JSONL note on
   the tip commit; create `refs/git-ticket/reviews/<id>` → tip commit.
3. `git ticket review comment <id> --file src/foo.rs --line 42 "..."` →
   append `CommentAdded` event to the same note.
4. `git ticket review show <id>` → replay events for that ID, compute diff
   (base..tip via `git2`), overlay comments at file/line, render.
5. `git ticket sync` → fetch, `git notes merge --strategy=cat_sort_uniq` on
   both notes refs, push all four ref patterns to the remote.

## Error Handling & Edge Cases

- **Branch rebased, root commit changes**: the ticket stays anchored to
  its *original* root commit via the pointer ref, so a later rebase
  doesn't move it. `git ticket list --branch <b>` checks whether the
  pointer ref's commit is still an ancestor of the branch tip; if not,
  the ticket is shown under an "orphaned/unlinked" grouping rather than
  silently disappearing.
- **Two branches share a merge-base**: handled by construction — see
  Ticket linkage above (separate event lines, retrieval by ID).
- **Partial sync failure leaves a pointer ref without a matching note (or
  vice versa)**: `git ticket doctor` scans for this, reports it, and can
  prune orphaned pointer refs on confirmation — never auto-deletes.
- **Concurrent sync from multiple clones**: safe by construction via
  `cat_sort_uniq`, *provided* every code path only ever appends to a note,
  never rewrites/replaces one. This "append is the only mutation" rule is
  the single most safety-critical invariant in the codebase and needs the
  most direct test coverage.
- **Empty/fresh repo with no commits** (matches this repo's current
  state): `git ticket new` requires at least one commit to anchor to and
  errors clearly rather than crashing.

## Testing Strategy

- **Unit tests**: event log append/replay/projection, ID generation,
  prefix resolution — pure logic in `git-ticket-core`, no CLI/network
  needed.
- **Integration tests**: real temp git repos (via `git2` or shelling out),
  simulate two clones pushing to a shared bare remote, run `sync`, assert
  conflict-free convergence to identical state. Highest-value tests given
  the architecture's central claim.
- **CLI smoke tests**: run the actual binary through a scripted scenario
  (branch → ticket → review → sync), assert output.
- **Web UI**: snapshot/HTML-assertion tests against a fixture repo state;
  no browser automation needed in v1 (read-only).

## Verification

- `cargo test` for unit + integration suites described above.
- Manual end-to-end walkthrough: `git init` a scratch repo, create a
  branch, `git ticket new`, `git ticket review start`, add comments,
  `git ticket review verdict`, `git ticket sync` against a local bare
  remote from a second clone, confirm convergence, `git ticket web` and
  visually confirm rendering.
