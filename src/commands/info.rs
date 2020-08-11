use anyhow::Result;

use crate::config;
use crate::database::Connection;
use crate::database_info::DatabaseInfo;

pub fn run() -> Result<()> {
    let db = Connection::open(config::database_path())?;
    db.create()?;
    let info: DatabaseInfo = db.into();
    println!("{}", info.format());
    Ok(())
}
