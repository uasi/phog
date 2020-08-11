use std::env;

use anyhow::Result;
use structopt::StructOpt;

use crate::commands;
use crate::config;

pub static APP_NAME: &str = structopt::clap::crate_name!();

pub fn run() -> Result<()> {
    let cli = Cli::new()?;
    cli.run()
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = APP_NAME,
    global_setting = structopt::clap::AppSettings::ColoredHelp,
    global_setting = structopt::clap::AppSettings::ColorAuto,
    global_setting = structopt::clap::AppSettings::VersionlessSubcommands
)]
pub struct Cli {
    #[structopt(subcommand)]
    command: Option<Command>,
}

impl Cli {
    pub fn new() -> Result<Self> {
        if env::args().count() < 2 {
            let mut args = vec![APP_NAME.to_owned()];
            if let Some(default_args) = config::settings()?.core.default_args {
                log::trace!("using default args; args={:?}", default_args);
                args.extend(default_args);
                return Ok(Self::from_iter(args));
            }
            args.push("--help".to_owned());
            Cli::clap().get_matches_from(args);
            unreachable!("get_matches_from will exit");
        }
        Ok(Self::from_args())
    }

    pub fn run(self) -> Result<()> {
        log::trace!("command: {:?}", self.command);
        if let Some(command) = self.command {
            return command.run();
        }
        Ok(())
    }
}

#[derive(Debug, StructOpt)]
enum Command {
    #[structopt(
        name = "download",
        about = "Downloads photos attached to the recorded tweets"
    )]
    Download(commands::download::Args),
    #[structopt(name = "forget", about = "Forgets recorded tweets and other data")]
    Forget(commands::forget::Args),
    #[structopt(name = "get", about = "Runs record and download at once")]
    Get(commands::get::Args),
    #[structopt(name = "info", about = "Prints the database info")]
    Info,
    #[structopt(name = "login", about = "Logs in to Twitter")]
    Login(commands::login::Args),
    #[structopt(name = "logout", about = "Logs out from Twitter")]
    Logout,
    #[structopt(name = "record", about = "Records tweets from various sources")]
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
