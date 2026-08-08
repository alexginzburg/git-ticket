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
git ticket new "Fix login bug" -b "Users can't log in on Safari"
git ticket list
git ticket status <id> in-progress
git ticket assign <id> alex

git ticket review start fix/login --base main
git ticket review comment <review-id> --file src/auth.rs --line 42 "why is this needed?"
git ticket review verdict <review-id> approve
git ticket review show <review-id>

git ticket web            # browse tickets/reviews at http://127.0.0.1:4747
```

Ticket/review ids are short hex strings, addressable by unambiguous prefix — you don't need to type the full id.

`git ticket new` also prints a suggested `Ticket-Id:` commit trailer you can add to commits on that branch, linking individual commits to the ticket in a way that's visible even without the tool installed.

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

Everyone on the team needs `git-ticket` installed locally (same `cargo install` step above) — the tool itself isn't distributed through the repo, only the ticket/review *data* is.

## Other commands

- `git ticket init` — explicitly configure the repo (idempotent; commands also self-initialize lazily on first use, so this is optional).
- `git ticket doctor [--prune]` — find pointer refs left dangling by a partial sync; `--prune` removes them (never runs automatically).

## Development

See [`CLAUDE.md`](CLAUDE.md) for build/test commands and architecture notes.

## License

Apache-2.0 — see [`LICENSE`](LICENSE).
