use crate::cli::ReviewAction;
use crate::git_env::{current_author, now_ts, open_repo};
use git_ticket_core::diff::{compute_diff, DiffLineKind};
use git_ticket_core::event::Verdict;
use git_ticket_core::review::ReviewState;
use git_ticket_core::review_service::{self, ReviewError};

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

fn print_error(e: ReviewError) -> ! {
    match e {
        ReviewError::NotFound => eprintln!("error: review not found"),
        ReviewError::Ambiguous(matches) => eprintln!("error: ambiguous id, matches: {}", matches.join(", ")),
        ReviewError::InvalidTarget(t) => eprintln!("error: could not resolve '{t}'"),
        ReviewError::Git(e) => eprintln!("error: {e}"),
    }
    std::process::exit(1);
}

fn print_summary(r: &ReviewState) {
    println!("{} target={} base={}", r.id, r.target, r.base);
}

pub fn run(action: ReviewAction) {
    let repo = match open_repo() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    let author = current_author(&repo);

    match action {
        ReviewAction::Start { target, base } => {
            let target = target.unwrap_or_else(|| "HEAD".to_string());
            match review_service::start_review(&repo, &target, base.as_deref(), &author, now_ts()) {
                Ok(r) => print_summary(&r),
                Err(e) => print_error(e),
            }
        }
        ReviewAction::Comment { review_id, file, line, text, reply_to } => {
            match review_service::add_comment(&repo, &review_id, &file, line, &text, reply_to.as_deref(), &author, now_ts()) {
                Ok(r) => print_summary(&r),
                Err(e) => print_error(e),
            }
        }
        ReviewAction::Verdict { review_id, verdict, summary } => {
            let verdict = match parse_verdict(&verdict) {
                Ok(v) => v,
                Err(msg) => {
                    eprintln!("error: {msg}");
                    std::process::exit(1);
                }
            };
            match review_service::set_verdict(&repo, &review_id, verdict, &author, now_ts()) {
                Ok(r) => {
                    print_summary(&r);
                    if let Some(s) = summary {
                        println!("{s}");
                    }
                }
                Err(e) => print_error(e),
            }
        }
        ReviewAction::Show { review_id } => match review_service::show_review(&repo, &review_id) {
            Ok(r) => {
                print_summary(&r);
                if let Ok((base_oid, target_oid)) = review_service::review_diff_range(&repo, &r) {
                    if let Ok(files) = compute_diff(&repo, base_oid, target_oid) {
                        for f in files {
                            println!("--- {}", f.path);
                            for h in f.hunks {
                                println!("{}", h.header);
                                for l in h.lines {
                                    let marker = match l.kind {
                                        DiffLineKind::Added => "+",
                                        DiffLineKind::Removed => "-",
                                        DiffLineKind::Context => " ",
                                    };
                                    println!("{marker}{}", l.content);
                                }
                            }
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
            Err(e) => print_error(e),
        },
    }
}
