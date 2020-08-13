use structopt::StructOpt;

use crate::commands;
use crate::result::*;

#[derive(Debug, StructOpt)]
pub struct Args {
    #[structopt(flatten)]
    pub download_args: commands::download::Args,
    #[structopt(flatten)]
    pub record_args: commands::record::Args,
}

pub fn run(args: Args) -> Result<()> {
    commands::record::run(args.record_args)?;
    commands::download::run(args.download_args)
}
