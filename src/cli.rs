use std::env;

use clap::Parser;

use crate::commands;
use crate::config;
use crate::result::*;

pub static APP_NAME: &str = clap::crate_name!();

pub fn run() -> Result<()> {
    let cli = Cli::new()?;
    cli.run()
}

#[derive(Debug, Parser)]
#[clap(name = APP_NAME)]
pub struct Cli {
    #[clap(subcommand)]
    command: Option<Command>,
}

impl Cli {
    pub fn new() -> Result<Self> {
        if env::args().count() < 2 {
            let mut args = vec![APP_NAME.to_owned()];
            if let Some(default_args) = config::settings()?.core.default_args {
                log::trace!("using default args; args={:?}", default_args);
                args.extend(default_args);
                return Ok(Self::parse_from(args));
            }
            args.push("--help".to_owned());
            Cli::parse_from(args);
            unreachable!("parse_from will exit because of --help");
        }
        Ok(Self::parse())
    }

    pub fn run(self) -> Result<()> {
        log::trace!("command: {:?}", self.command);
        if let Some(command) = self.command {
            return command.run();
        }
        Ok(())
    }
}

#[derive(Debug, Parser)]
enum Command {
    #[clap(about = "Downloads photos attached to the recorded tweets")]
    Download(commands::download::Args),
    #[clap(about = "Forgets recorded tweets and other data")]
    Forget(commands::forget::Args),
    #[clap(about = "Runs record and download at once")]
    Get(commands::get::Args),
    #[clap(about = "Prints the database info")]
    Info,
    #[clap(about = "Logs in to Twitter")]
    Login(commands::login::Args),
    #[clap(about = "Logs out from Twitter")]
    Logout,
    #[clap(about = "Records tweets from various sources")]
    Record(commands::record::Args),
}

impl Command {
    pub fn run(self) -> Result<()> {
        use commands::*;
        match self {
            Self::Download(args) => download::run(args),
            Self::Forget(args) => forget::run(args),
            Self::Get(args) => get::run(args),
            Self::Info => info::run(),
            Self::Login(args) => login::run(args),
            Self::Logout => logout::run(),
            Self::Record(args) => commands::record::run(args),
        }
    }
}
