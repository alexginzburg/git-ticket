# git-ticket

[![Rust](https://github.com/alexginzburg/ticket/actions/workflows/rust.yml/badge.svg?branch=main)](https://github.com/alexginzburg/ticket/actions/workflows/rust.yml)

A ticketing and code-review tool that lives entirely inside git. Ticket bodies and review comments are stored as git notes/refs — no server, no database, no SaaS account. They travel with the code over ordinary `git push`/`fetch`/`clone`, and every command except `sync` works fully offline.

See [`docs/superpowers/specs/2026-08-06-git-ticket-design.md`](docs/superpowers/specs/2026-08-06-git-ticket-design.md) for the full design rationale.

## Install

```bash
cargo install --path crates/git-ticket-cli
```

This installs a `git-ticket` binary on your `PATH`. Because git treats any `git-<name>` executable on `PATH` as a subcommand, `git ticket ...` works automatically from inside any repo — no further setup required.

## Quick tour

```bash
git checkout -b fix/login
git ticket new "Fix login bug" -b "Users can't log in on Safari" --type bug
git ticket list                      # defaults to open tickets; --status all shows everything
git ticket show <id>                 # body + comments
git ticket status <id> in-progress
git ticket type <id> feature
git ticket assign <id> alex
git ticket comment <id> "still reproduces on latest main"

git ticket review start fix/login --base main
git ticket review comment <review-id> --file src/auth.rs --line 42 "why is this needed?"
git ticket review verdict <review-id> approve
git ticket review show <review-id>

git ticket web             # prints a clickable URL, e.g. http://localhost:4747/git-ticket
```

Ticket/review ids are short hex strings, addressable by unambiguous prefix — you don't need to type the full id.

`git ticket new` also prints a suggested `Ticket-Id:` commit trailer you can add to commits on that branch, linking individual commits to the ticket in a way that's visible even without the tool installed.

Tickets have a `type` (`task`/`bug`/`feature`/`chore`, defaulting to `task`) alongside their `status`. Both `list` and the web ticket list filter to `status: open` by default — pass `--status all` (CLI) or `?status=all` (web) to see closed/in-progress tickets too, or `--status closed`/`?status=closed` etc. to see just one status.

`git ticket web` binds port 4747 by default, but falls back to a free port automatically if that's already taken (e.g. by another repo's `git ticket web`), and the URL it prints always includes the repo's name in the path — so you can run it in several repos at once and tell their browser tabs apart.

## Using it with a team

Every command above is local-only. To share tickets/reviews with collaborators:

```bash
git ticket sync            # fetch + merge + push, defaults to the `origin` remote
```

This pushes/fetches four ref namespaces alongside your normal git push/fetch:
```
refs/notes/git-ticket/tickets     refs/notes/git-ticket/reviews
refs/git-ticket/tickets/<id>      refs/git-ticket/reviews/<id>
```

Merges are conflict-free by construction (an append-only event log, unioned via a `cat_sort_uniq`-equivalent merge) — concurrent edits from different people never produce a real git merge conflict. If a push briefly loses a race with another sync, `git ticket sync` retries a bounded number of times; if it still fails, just run it again.

`sync` authenticates against the remote the same way `git push`/`fetch` do (SSH agent, then your git credential helper, then libgit2's default) — no separate login step. It reports what it actually did, e.g. `synced: 2 ref(s) pushed, 0 ticket note(s) merged, 0 review note(s) merged` — a run that only pushed your own changes (nothing new to merge in) is expected to show `0` merged and isn't a sign that sync did nothing.

Everyone on the team needs `git-ticket` installed locally (same `cargo install` step above) — the tool itself isn't distributed through the repo, only the ticket/review *data* is.

## Other commands

- `git ticket init` — explicitly configure the repo (idempotent; commands also self-initialize lazily on first use, so this is optional).
- `git ticket doctor [--prune]` — find pointer refs left dangling by a partial sync; `--prune` removes them (never runs automatically).

## Viewing tickets/reviews alongside `git log`

`git ticket log [<revspec>]` walks commit history (defaulting to `HEAD`, or a given revspec) and prints a decoration line under any commit that's the anchor of a ticket or review, showing its *current* projected state (not just its state at creation):

```
00dbe55 feature work
        [TICKET-a1b2c3d4 "Fix login bug" status=open type=bug]
        [REVIEW-9f8e7d6c target=00dbe55 base=main verdict=pending]
```

A ticket decorates the merge-base commit of its branch; a review decorates the exact commit snapshotted when it was opened — see `CLAUDE.md` for why.

For raw access to the underlying event log, `git ticket init` (and lazy first use) also adds `refs/notes/git-ticket/tickets` and `refs/notes/git-ticket/reviews` to your repo's local `notes.displayRef` config, so plain `git log` shows the raw JSONL events on those same commits without any extra flags. That's a local-only setting — `git config --unset-all notes.displayRef` turns it back off if you find it noisy.

## Development

See [`CLAUDE.md`](CLAUDE.md) for build/test commands and architecture notes.

## License

Apache-2.0 — see [`LICENSE`](LICENSE).
