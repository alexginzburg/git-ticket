mod cli;
mod commands;
mod git_env;

use clap::Parser;
use cli::{Cli, Command, OutputFormat, ReviewAction, TicketAction};

fn command_supports_json(command: &Command) -> bool {
    matches!(
        command,
        Command::List { .. } | Command::Show { .. } | Command::Review { action: ReviewAction::Show { .. } }
    )
}

fn main() {
    let cli = Cli::parse();
    if cli.format == OutputFormat::Json && !command_supports_json(&cli.command) {
        commands::output::error_exit(cli.format, "--format json is not supported for this command");
    }

    match cli.command {
        Command::Init => commands::init::run(),
        Command::New { title, assignee, body, ticket_type } => {
            commands::ticket::run(TicketAction::New { title, assignee, body, ticket_type }, cli.format)
        }
        Command::List { branch, status, assignee, ticket_type } => {
            commands::ticket::run(TicketAction::List { branch, status, assignee, ticket_type }, cli.format)
        }
        Command::Show { id } => commands::ticket::run(TicketAction::Show { id }, cli.format),
        Command::Status { id, status } => {
            commands::ticket::run(TicketAction::Status { id, status }, cli.format)
        }
        Command::Type { id, ticket_type } => {
            commands::ticket::run(TicketAction::Type { id, ticket_type }, cli.format)
        }
        Command::Assign { id, assignee } => {
            commands::ticket::run(TicketAction::Assign { id, assignee }, cli.format)
        }
        Command::Comment { id, text } => {
            commands::ticket::run(TicketAction::Comment { id, text }, cli.format)
        }
        Command::Review { action } => commands::review::run(action, cli.format),
        Command::Sync { remote } => commands::sync::run(remote),
        Command::Doctor { prune } => commands::doctor::run(prune),
        Command::Web { port } => commands::web::run(port),
        Command::Log { revspec } => commands::log::run(revspec),
    }
}
