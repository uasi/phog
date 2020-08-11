use std::fs;
use std::path::Path;

use indicatif::HumanBytes;
use rusqlite::params;

use crate::config;
use crate::database::Connection;

pub struct DatabaseInfo {
    conn: rusqlite::Connection,
}

impl DatabaseInfo {
    pub fn format(&self) -> String {
        let path = config::database_path();

        format!(
            "\
            DB path        : {path:?}\n\
            DB size        : {size}\n\
            Tweets         : {tweets}\n\
            Pruned tweets  : {pruned_tweets}\
            ",
            path = path,
            size = file_size(&path),
            tweets = self.tweets(),
            pruned_tweets = self.pruned_tweets(),
        )
    }

    fn tweets(&self) -> String {
        self.conn
            .query_row("SELECT COUNT(*) FROM tweets;", params![], |row| {
                row.get(0).map(|i: i64| i.to_string())
            })
            .unwrap_or_else(|e| format!("(Error: {:?})", e))
    }

    fn pruned_tweets(&self) -> String {
        self.conn
            .query_row("SELECT COUNT(*) FROM pruned_tweets;", params![], |row| {
                row.get(0).map(|i: i64| i.to_string())
            })
            .unwrap_or_else(|e| format!("(Error: {:?})", e))
    }
}

impl From<Connection> for DatabaseInfo {
    fn from(conn: Connection) -> Self {
        DatabaseInfo { conn: conn.into() }
    }
}

fn file_size(path: &Path) -> String {
    match fs::metadata(path) {
        Ok(metadata) => HumanBytes(metadata.len()).to_string(),
        _ => "(Unknown)".to_owned(),
    }
}
