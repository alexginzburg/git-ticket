mod cli;
mod commands;
mod git_env;

use clap::Parser;
use cli::{Cli, Command, TicketAction};

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Init => commands::init::run(),
        Command::New { title, assignee, body } => {
            commands::ticket::run(TicketAction::New { title, assignee, body })
        }
        Command::List { branch, status, assignee } => {
            commands::ticket::run(TicketAction::List { branch, status, assignee })
        }
        Command::Show { id } => commands::ticket::run(TicketAction::Show { id }),
        Command::Status { id, status } => {
            commands::ticket::run(TicketAction::Status { id, status })
        }
        Command::Assign { id, assignee } => {
            commands::ticket::run(TicketAction::Assign { id, assignee })
        }
        Command::Comment { id, text } => commands::ticket::run(TicketAction::Comment { id, text }),
        Command::Review { action } => commands::review::run(action),
        Command::Sync { remote } => commands::sync::run(remote),
        Command::Doctor { prune } => commands::doctor::run(prune),
        Command::Web { port } => commands::web::run(port),
    }
}
