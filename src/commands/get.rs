use anyhow::Result;
use structopt::StructOpt;

use crate::commands;

#[derive(Debug, StructOpt)]
pub struct Args {
    #[structopt(flatten)]
    pub download_args: commands::download::Args,
    #[structopt(flatten)]
    pub record_args: commands::record::Args,
}

pub fn run(args: Args) -> Result<()> {
    if !args.record_args.is_empty() {
        commands::record::run(args.record_args)?;
    }
    commands::download::run(args.download_args)
}
