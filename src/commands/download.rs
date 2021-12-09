use std::path::PathBuf;

use clap::Parser;

use crate::commands;
use crate::common::count;
use crate::config;
use crate::database::Connection;
use crate::downloader::{build_photo_path, Downloader};
use crate::result::*;

static AUTO_GC_THRESHOLD: u64 = 4096;

#[derive(Debug, Parser)]
pub struct Args {
    #[clap(long, help = "Sets download directory")]
    pub dir: Option<PathBuf>,
}

pub fn run(args: Args) -> Result<()> {
    let dir = set_download_dir(args.dir)?;
    println!("Downloading photos to {:?}.", dir);

    let db = Connection::open(config::database_path())?;
    db.create()?;

    let photosets = db.select_not_downloaded_photos()?;

    if photosets.is_empty() {
        println!("No photos to download.");
        run_gc_if_needed(db.count_tweets()?)?;
        return Ok(());
    }

    println!("Downloading {}.", count(photosets.len(), "photoset"));

    let downloader = Downloader::new(
        photosets,
        Box::new(move |photoset| {
            for (index, photo_url) in (1..).zip(photoset.photo_urls.iter()) {
                let path = build_photo_path(photoset, photo_url, index);
                println!("Downloaded {}", path.to_string_lossy());
            }
            if let Err(e) = db.set_photos_downloaded_at(photoset.rowid) {
                log::debug!("set_photos_downloaded_at failed; error={:?}", e);
                eprintln!(
                    "Warning: Failed to mark photoset as downloaded. (status_id = {})",
                    photoset.id_str
                );
            }
        }),
    );
    downloader.start()?;

    println!("Done.");

    run_gc_if_needed(Connection::open(config::database_path())?.count_tweets()?)?;

    Ok(())
}

fn set_download_dir(dir_arg: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(dir) = dir_arg.or_else(|| config::settings().ok().and_then(|s| s.download.dir)) {
        if !dir.is_dir() {
            bail!("The download directory does not exist: {:?}", &dir);
        }
        log::trace!("chdir to {:?}", &dir);
        std::env::set_current_dir(&dir)?;
        return Ok(dir);
    }

    Ok(std::env::current_dir()?)
}

fn run_gc_if_needed(tweets: u64) -> Result<()> {
    log::trace!(
        "checking if gc is needed; tweets={}, threshold={}",
        tweets,
        AUTO_GC_THRESHOLD
    );
    if tweets >= AUTO_GC_THRESHOLD {
        commands::forget::run_gc()?;
    }
    Ok(())
}
