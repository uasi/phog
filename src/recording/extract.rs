use std::io::{self, Read};

use anyhow::{Context, Result};

use crate::clipboard;
use crate::database::Connection;
use crate::recording::record;

pub struct Extract<'a> {
    pub db: &'a Connection,
}

impl<'a> Extract<'a> {
    pub fn new(db: &'a Connection) -> Self {
        Self { db }
    }

    pub fn from_clipboard_watcher(&self) -> Result<()> {
        println!("Watching the clipboard for tweet URLs... (Ctrl-C to stop)");
        let changes_rx = clipboard::spawn_watcher();
        loop {
            if let Some(text) = changes_rx.recv().expect("recv must succeed") {
                record::with_string(self.db, text)?;
            } else {
                println!("Stopped.");
                break;
            }
        }
        Ok(())
    }

    pub fn from_clipboard(&self) -> Result<()> {
        log::trace!("extracting from clipboard");
        record::with_string(self.db, clipboard::read()?)
    }

    pub fn from_stdin(&self) -> Result<()> {
        if atty::is(atty::Stream::Stdin) {
            log::trace!("skipping extracting from stdin; stdin=tty");
            Ok(())
        } else {
            log::trace!("extracting from stdin; stdin=!tty");
            record::with_string(self.db, read_from_stdin()?)
        }
    }
}

fn read_from_stdin() -> Result<String> {
    let mut buf = String::new();
    io::stdin()
        .read_to_string(&mut buf)
        .context("Could not read from stdin")?;
    Ok(buf)
}
