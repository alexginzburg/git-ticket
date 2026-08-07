mod cli;
mod commands;
mod git_env;

use clap::Parser;
use cli::{Cli, Command};

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Init => commands::init::run(),
        Command::Ticket { action } => commands::ticket::run(action),
        Command::Review { action } => commands::review::run(action),
        Command::Sync { remote } => commands::sync::run(remote),
        Command::Doctor { prune } => commands::doctor::run(prune),
        Command::Web { port } => commands::web::run(port),
    }
}
