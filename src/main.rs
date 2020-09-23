mod cli;
mod clipboard;
mod commands;
mod common;
mod config;
mod database;
mod database_info;
mod downloader;
mod egg_mode_ext;
mod input;
mod recording;
mod result;
mod spinner;
mod twitter;

fn main() -> result::Result<()> {
    color_eyre::install()?;
    pretty_env_logger::init_timed();
    config::init()?;
    smol::block_on(async_compat::Compat::new(async { cli::run() }))
}
