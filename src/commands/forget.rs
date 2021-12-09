use clap::Parser;

use crate::common::count;
use crate::config;
use crate::database::Connection;
use crate::result::*;

#[derive(Debug, Parser)]
#[clap(setting = clap::AppSettings::ArgRequiredElseHelp)]
pub struct Args {
    #[clap(long, help = "Performs housekeeping on the database")]
    pub gc: bool,
}

pub fn run(args: Args) -> Result<()> {
    if args.gc {
        run_gc()
    } else {
        unreachable!("arg required");
    }
}

pub fn run_gc() -> Result<()> {
    let db = Connection::open(config::database_path())?;
    db.create()?;

    let n = db.prune_tweets()?;
    println!("Pruned {}.", count(n, "tweet"));

    if n > 0 {
        db.vacuum()?;
        println!("Vacuumed database.");
    }

    Ok(())
}
