mod cli;
mod commands;
mod git_env;

use clap::Parser;
use cli::{Cli, Command};

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Ticket { action } => commands::ticket::run(action),
    }
}
