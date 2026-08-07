use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "git-ticket")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Ticket {
        #[command(subcommand)]
        action: TicketAction,
    },
    Review {
        #[command(subcommand)]
        action: ReviewAction,
    },
    Sync {
        remote: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum TicketAction {
    New {
        title: String,
        #[arg(short = 'a', long)]
        assignee: Option<String>,
        #[arg(short = 'b', long, default_value = "")]
        body: String,
    },
    List {
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        assignee: Option<String>,
    },
    Show {
        id: String,
    },
    Status {
        id: String,
        status: String,
    },
    Assign {
        id: String,
        assignee: String,
    },
    Comment {
        id: String,
        text: String,
    },
}

#[derive(Subcommand)]
pub enum ReviewAction {
    Start {
        target: Option<String>,
        #[arg(long)]
        base: Option<String>,
    },
    Comment {
        review_id: String,
        #[arg(long)]
        file: String,
        #[arg(long)]
        line: u32,
        text: String,
        #[arg(long)]
        reply_to: Option<String>,
    },
    Verdict {
        review_id: String,
        verdict: String,
        summary: Option<String>,
    },
    Show {
        review_id: String,
    },
}
