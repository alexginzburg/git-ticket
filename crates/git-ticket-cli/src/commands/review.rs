use crate::cli::{OutputFormat, ReviewAction};
use crate::commands::output;
use crate::git_env::{current_author, now_ts, open_repo};
use git_ticket_core::diff::{compute_diff, DiffLineKind, FileDiff};
use git_ticket_core::event::Verdict;
use git_ticket_core::review::ReviewState;
use git_ticket_core::review_service::{self, ReviewError};
use serde::Serialize;

fn parse_verdict(s: &str) -> Result<Verdict, String> {
    match s {
        "approve" => Ok(Verdict::Approve),
        "request-changes" => Ok(Verdict::RequestChanges),
        "comment" => Ok(Verdict::Comment),
        other => Err(format!("invalid verdict '{other}', expected approve|request-changes|comment")),
    }
}

fn verdict_str(v: &Verdict) -> &'static str {
    match v {
        Verdict::Approve => "approve",
        Verdict::RequestChanges => "request-changes",
        Verdict::Comment => "comment",
    }
}

fn review_error_message(e: &ReviewError) -> String {
    match e {
        ReviewError::NotFound => "review not found".to_string(),
        ReviewError::Ambiguous(matches) => format!("ambiguous id, matches: {}", matches.join(", ")),
        ReviewError::InvalidTarget(t) => format!("could not resolve '{t}'"),
        ReviewError::Git(e) => format!("{e}"),
    }
}

fn print_error(format: OutputFormat, e: ReviewError) -> ! {
    output::error_exit(format, &review_error_message(&e))
}

fn print_summary(r: &ReviewState) {
    println!("{} target={} base={}", r.id, r.target, r.base);
}

/// `ReviewState`'s own fields flattened alongside the computed diff -- a
/// review's diff isn't part of its persisted state (it's recomputed from
/// the snapshotted target/base each time), so it's not a `ReviewState`
/// field, just added here for the JSON view.
#[derive(Serialize)]
struct ReviewShowJson {
    #[serde(flatten)]
    review: ReviewState,
    files: Vec<FileDiff>,
}

pub fn run(action: ReviewAction, format: OutputFormat) {
    let repo = match open_repo() {
        Ok(r) => r,
        Err(e) => output::error_exit(format, &e),
    };
    let author = current_author(&repo);

    match action {
        ReviewAction::Start { target, base } => {
            let target = target.unwrap_or_else(|| "HEAD".to_string());
            match review_service::start_review(&repo, &target, base.as_deref(), &author, now_ts()) {
                Ok(r) => print_summary(&r),
                Err(e) => print_error(format, e),
            }
        }
        ReviewAction::Comment { review_id, file, line, text, reply_to } => {
            match review_service::add_comment(&repo, &review_id, &file, line, &text, reply_to.as_deref(), &author, now_ts()) {
                Ok(r) => print_summary(&r),
                Err(e) => print_error(format, e),
            }
        }
        ReviewAction::Verdict { review_id, verdict, summary } => {
            let verdict = match parse_verdict(&verdict) {
                Ok(v) => v,
                Err(msg) => output::error_exit(format, &msg),
            };
            match review_service::set_verdict(&repo, &review_id, verdict, &author, now_ts()) {
                Ok(r) => {
                    print_summary(&r);
                    if let Some(s) = summary {
                        println!("{s}");
                    }
                }
                Err(e) => print_error(format, e),
            }
        }
        ReviewAction::Show { review_id } => match review_service::show_review(&repo, &review_id) {
            Ok(r) => {
                let files = review_service::review_diff_range(&repo, &r)
                    .ok()
                    .and_then(|(base_oid, target_oid)| compute_diff(&repo, base_oid, target_oid).ok())
                    .unwrap_or_default();

                match format {
                    OutputFormat::Json => output::print_json(&ReviewShowJson { review: r, files }),
                    OutputFormat::Text => {
                        print_summary(&r);
                        for f in &files {
                            println!("--- {}", f.path);
                            for h in &f.hunks {
                                println!("{}", h.header);
                                for l in &h.lines {
                                    let marker = match l.kind {
                                        DiffLineKind::Added => "+",
                                        DiffLineKind::Removed => "-",
                                        DiffLineKind::Context => " ",
                                    };
                                    println!("{marker}{}", l.content);
                                }
                            }
                        }
                        for c in &r.comments {
                            println!("  [{}:{}] {} ({}): {}", c.file, c.line, c.author, c.ts, c.body);
                        }
                        if let Some((author, verdict, _)) = r.latest_verdict() {
                            println!("verdict: {} by {author}", verdict_str(verdict));
                        }
                    }
                }
            }
            Err(e) => print_error(format, e),
        },
    }
}
