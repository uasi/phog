use clap::Parser;

use crate::commands;
use crate::result::*;

#[derive(Debug, Parser)]
pub struct Args {
    #[clap(flatten)]
    pub download_args: commands::download::Args,
    #[clap(flatten)]
    pub record_args: commands::record::Args,
}

pub fn run(args: Args) -> Result<()> {
    commands::record::run(args.record_args)?;
    commands::download::run(args.download_args)
}
