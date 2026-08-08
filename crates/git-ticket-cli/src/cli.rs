use clap::{Parser, Subcommand, ValueEnum};

#[derive(Clone, ValueEnum)]
pub enum TicketTypeArg {
    Task,
    Bug,
    Feature,
    Chore,
}

#[derive(Parser)]
#[command(name = "git-ticket")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

// Ticket actions are flattened to top-level subcommands (`git-ticket new`,
// not `git-ticket ticket new`) because `git ticket new` already strips the
// `ticket` word before invoking this binary — git resolves `git ticket` to
// running `git-ticket` and forwards only the remaining args.
#[derive(Subcommand)]
pub enum Command {
    Init,
    New {
        title: String,
        #[arg(short = 'a', long)]
        assignee: Option<String>,
        #[arg(short = 'b', long, default_value = "")]
        body: String,
        #[arg(long = "type", default_value = "task")]
        ticket_type: TicketTypeArg,
    },
    List {
        #[arg(long)]
        branch: Option<String>,
        #[arg(long, default_value = "open")]
        status: String,
        #[arg(long)]
        assignee: Option<String>,
        #[arg(long = "type")]
        ticket_type: Option<TicketTypeArg>,
    },
    Show {
        id: String,
    },
    Status {
        id: String,
        status: String,
    },
    Type {
        id: String,
        ticket_type: TicketTypeArg,
    },
    Assign {
        id: String,
        assignee: String,
    },
    Comment {
        id: String,
        text: String,
    },
    Review {
        #[command(subcommand)]
        action: ReviewAction,
    },
    Sync {
        remote: Option<String>,
    },
    Doctor {
        #[arg(long)]
        prune: bool,
    },
    Web {
        #[arg(long)]
        port: Option<u16>,
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
        #[arg(long = "type", default_value = "task")]
        ticket_type: TicketTypeArg,
    },
    List {
        #[arg(long)]
        branch: Option<String>,
        #[arg(long, default_value = "open")]
        status: String,
        #[arg(long)]
        assignee: Option<String>,
        #[arg(long = "type")]
        ticket_type: Option<TicketTypeArg>,
    },
    Show {
        id: String,
    },
    Status {
        id: String,
        status: String,
    },
    Type {
        id: String,
        ticket_type: TicketTypeArg,
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
