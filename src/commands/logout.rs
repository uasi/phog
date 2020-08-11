use std::fs;

use anyhow::Result;

use crate::config;

pub fn run() -> Result<()> {
    let mut removed_any = false;

    let path = config::access_token_path();
    if path.exists() {
        fs::remove_file(&path)?;
        log::trace!("removed {:?}", &path);
        removed_any = true;
    }

    let path = config::credentials_path();
    if path.exists() {
        fs::remove_file(&path)?;
        log::trace!("removed {:?}", &path);
        removed_any = true;
    }

    if removed_any {
        println!("Logged out successfully.");
    } else {
        println!("Not logged in.");
    }

    Ok(())
}
