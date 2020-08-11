use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use anyhow::{bail, Result};
use rusqlite::{named_params, params};
use serde::Deserialize;

use crate::twitter::Tweet;

static SCHEMA_SQL: &str = include_str!("../data/schema.sql");

pub struct Connection {
    conn: rusqlite::Connection,
}

impl Connection {
    pub fn open<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let path = path.as_ref();
        log::trace!("opening database at {:?}", path);
        fs::create_dir_all(path.parent().expect("database path must have base dir"))?;
        let conn = rusqlite::Connection::open(path)?;
        log::trace!("opened database");
        Ok(Connection { conn })
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = rusqlite::Connection::open_in_memory()?;
        Ok(Connection { conn })
    }

    #[cfg(test)]
    pub fn inner(&self) -> &rusqlite::Connection {
        &self.conn
    }

    pub fn create(&self) -> Result<()> {
        self.conn.execute_batch(SCHEMA_SQL)?;
        log::trace!("created tables");
        Ok(())
    }

    pub fn count_tweets(&self) -> Result<u64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM tweets;", params![], |row| row.get(0))?;
        Ok(count as u64)
    }

    pub fn insert_loose_tweets(&self, tweets: &[Tweet]) -> Result<usize> {
        self.conn.execute("BEGIN;", params![])?;
        let inserted = self.insert_tweets(tweets, false)?;
        log::trace!("inserted unseen loose tweets; n={}", inserted);
        self.conn.execute("COMMIT;", params![])?;
        Ok(inserted)
    }

    pub fn insert_timeline_tweets(&self, tweets: &[Tweet]) -> Result<usize> {
        let mut update_tweet_stmt = self.conn.prepare(
            r#"
            UPDATE tweets SET in_timeline = 1 WHERE status_id = ?;
            "#,
        )?;
        let mut update_pruned_tweet_stmt = self.conn.prepare(
            r#"
            UPDATE pruned_tweets SET in_timeline = 1 WHERE status_id = ?;
            "#,
        )?;

        self.conn.execute("BEGIN;", params![])?;

        for tweet in tweets {
            update_tweet_stmt.execute(params![tweet.id.to_string()])?;
            update_pruned_tweet_stmt.execute(params![tweet.id.to_string()])?;
        }
        log::trace!(
            "updated in_timeline for tweets and pruned_tweets; n={}",
            tweets.len()
        );

        let inserted = self.insert_tweets(&tweets, true)?;
        log::trace!("inserted unseen timeline tweets; n={}", inserted);

        self.conn.execute("COMMIT;", params![])?;

        Ok(inserted)
    }

    fn insert_tweets(&self, tweets: &[Tweet], in_timeline: bool) -> Result<usize> {
        fn take_unseen_tweets<'a>(
            conn: &Connection,
            tweets: &'a [Tweet],
        ) -> Result<impl Iterator<Item = &'a Tweet>> {
            let status_ids: Vec<u64> = tweets.iter().map(|tweet| tweet.id).collect();
            let unseen_status_ids: HashSet<u64> = conn
                .select_unseen_status_ids_from(&status_ids)?
                .into_iter()
                .collect();
            let tweets = tweets
                .iter()
                .filter(move |tweet| unseen_status_ids.contains(&tweet.id));
            Ok(tweets)
        }

        let mut stmt = self.conn.prepare(
            r#"
            INSERT OR IGNORE INTO tweets (status_id, content, in_timeline, recorded_at)
            VALUES (?, ?, ?, ?);
            "#,
        )?;

        let recorded_at: String =
            self.conn
                .query_row("SELECT CURRENT_TIMESTAMP;", params![], |row| row.get(0))?;

        let mut inserted = 0;
        for tweet in take_unseen_tweets(self, tweets)? {
            inserted += stmt.execute(params![
                tweet.id.to_string(),
                tweet.json,
                in_timeline,
                recorded_at
            ])?;
        }

        Ok(inserted)
    }

    pub fn prune_tweets(&self) -> Result<usize> {
        struct Row {
            status_id: String,
            user_id: String,
            screen_name: String,
            media: Option<String>,
            in_timeline: bool,
            recorded_at: Option<String>,
            photos_downloaded_at: Option<String>,
        }

        // Returns true is the row has no media,
        // or the media contains no photos,
        // or the photos are already downloaded.
        fn is_prunable_row(row: &Row) -> bool {
            match row.media {
                None => true,
                Some(ref media) => match serde_json::from_str::<Option<Vec<MediaEntity>>>(media) {
                    Err(_e) => {
                        if cfg!(test) {
                            panic!("media entity is malformed: {:?}", _e);
                        } else {
                            false
                        }
                    }
                    Ok(None) => true,
                    Ok(Some(media_entities)) => {
                        if media_entities.iter().any(|m| m.type_ == "photo") {
                            row.photos_downloaded_at.is_some()
                        } else {
                            true
                        }
                    }
                },
            }
        }

        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                status_id,
                json_extract(tweets.content, '$.user.id_str') AS user_id,
                json_extract(tweets.content, '$.user.screen_name') AS screen_name,
                IFNULL(
                    json_extract(tweets.content, '$.extended_entities.media'),
                    json_quote(json_extract(tweets.content, '$.extended_entities.media'))
                ) AS media,
                in_timeline,
                recorded_at,
                photos_downloaded_at
            FROM tweets
            ORDER BY id;
            "#,
        )?;
        let rows = stmt.query_map(params![], |row| {
            Ok(Row {
                status_id: row.get_unwrap("status_id"),
                user_id: row.get_unwrap("user_id"),
                screen_name: row.get_unwrap("screen_name"),
                media: row.get_unwrap("media"),
                in_timeline: row.get_unwrap("in_timeline"),
                recorded_at: row.get_unwrap("recorded_at"),
                photos_downloaded_at: row.get_unwrap("photos_downloaded_at"),
            })
        })?;

        let pruned_at: String =
            self.conn
                .query_row("SELECT CURRENT_TIMESTAMP;", params![], |row| row.get(0))?;

        let mut insert_stmt = self.conn.prepare(
            r#"
            INSERT OR IGNORE INTO pruned_tweets (
                status_id,
                user_id,
                screen_name,
                media,
                in_timeline,
                recorded_at,
                photos_downloaded_at,
                pruned_at
            )
            VALUES (
                :status_id,
                :user_id,
                :screen_name,
                :media,
                :in_timeline,
                :recorded_at,
                :photos_downloaded_at,
                :pruned_at
            );
            "#,
        )?;
        let mut delete_stmt = self.conn.prepare(
            r#"
            DELETE FROM tweets WHERE status_id = ?;
            "#,
        )?;

        self.conn.execute("BEGIN;", params![])?;
        let mut pruned = 0;
        for row in rows.flatten() {
            if is_prunable_row(&row) {
                insert_stmt.execute_named(named_params! {
                    ":status_id": row.status_id,
                    ":user_id": row.user_id,
                    ":screen_name": row.screen_name,
                    ":media": row.media,
                    ":in_timeline": row.in_timeline,
                    ":recorded_at": row.recorded_at,
                    ":photos_downloaded_at": row.photos_downloaded_at,
                    ":pruned_at": pruned_at
                })?;
                delete_stmt.execute(params![row.status_id])?;
                pruned += 1;
            }
        }
        self.conn.execute("COMMIT;", params![])?;

        Ok(pruned)
    }

    pub fn select_max_status_id(&self, user_id: u64) -> Result<Option<String>> {
        // We can't use `SELECT MAX(status_id AS INTEGER)` because status_id may not be convertible to (64-bit signed) INTEGER.
        let mut stmt = self.conn.prepare(
            r#"
            SELECT status_id FROM seen_tweets WHERE user_id = ? AND in_timeline = 1;
            "#,
        )?;
        let rows = stmt.query_map(params![user_id.to_string()], |row| {
            let status_id: String = row.get_unwrap(0);
            Ok(status_id)
        })?;

        let max: Option<(String, u64)> = rows
            .flatten()
            .filter_map(|status_id| u64::from_str(&status_id).ok().map(|key| (status_id, key)))
            .max_by_key(|(_, key)| *key);

        Ok(max.map(|(status_id, _)| status_id))
    }

    pub fn select_not_downloaded_photos(&self) -> Result<Vec<Photoset>> {
        #[derive(Eq, Ord, PartialEq, PartialOrd)]
        struct Row {
            rowid: i64,
            screen_name: String,
            id_str: String,
            media_json: String,
        }

        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                rowid,
                json_extract(tweets.content, '$.user.screen_name'),
                json_extract(tweets.content, '$.id_str'),
                json_quote(json_extract(tweets.content, '$.extended_entities.media'))
            FROM tweets
            WHERE tweets.photos_downloaded_at IS NULL;
            "#,
        )?;
        let rows = stmt.query_map(params![], |row| {
            // Use unwrap here to panic if there is data inconsistency.
            let rowid = row.get_unwrap(0);
            let screen_name = row.get_unwrap(1);
            let id_str = row.get_unwrap(2);
            let media_json = row.get_unwrap(3);
            Ok(Row {
                rowid,
                screen_name,
                id_str,
                media_json,
            })
        })?;

        let mut photosets = vec![];

        for row in rows.flatten() {
            match build_photoset(row.rowid, row.screen_name, row.id_str, row.media_json) {
                Ok(Some(photoset)) => photosets.push(photoset),
                Ok(None) => (),
                Err(e) => return Err(e),
            }
        }

        Ok(photosets)
    }

    pub fn select_unseen_status_ids_from(&self, status_ids: &[u64]) -> Result<Vec<u64>> {
        if status_ids.is_empty() {
            return Ok(vec![]);
        }

        let _handle =
            self.create_autodropping_temp_table("status_ids", "status_id TEXT NOT NULL")?;

        {
            let mut insert_stmt = self.conn.prepare(
                r#"
                INSERT OR IGNORE INTO temp.status_ids VALUES (?);
                "#,
            )?;
            for status_id in status_ids {
                insert_stmt.execute(&[<u64>::to_string(status_id)])?;
            }
        }

        let mut stmt = self.conn.prepare(
            r#"
            SELECT status_id FROM temp.status_ids EXCEPT SELECT status_id FROM seen_tweets;
            "#,
        )?;

        let rows = stmt.query_map(params![], |row| row.get(0))?.flatten();
        Ok(rows.map(|s: String| u64::from_str(&s)).flatten().collect())
    }

    pub fn set_photos_downloaded_at(&self, rowid: i64) -> Result<usize> {
        let n = self.conn.execute(
            r#"
            UPDATE tweets
            SET photos_downloaded_at = CURRENT_TIMESTAMP
            WHERE rowid = ?;
            "#,
            params![rowid],
        )?;
        log::trace!("set photo_downloaded_at; rowid={}", rowid);
        Ok(n)
    }

    pub fn vacuum(&self) -> Result<()> {
        self.conn.execute("VACUUM;", params![])?;
        Ok(())
    }

    fn create_autodropping_temp_table<'a>(
        &'a self,
        table_name: &str,
        columns: &str,
    ) -> Result<TempTableDropHandle<'a>> {
        log::trace!(
            "creating temp table; table_name={}, columns=({})",
            table_name,
            columns
        );

        self.conn.execute_batch(&format!(
            r#"
            DROP TABLE IF EXISTS temp.{table_name};
            CREATE TABLE temp.{table_name} ({columns});
            "#,
            table_name = table_name,
            columns = columns
        ))?;

        Ok(TempTableDropHandle {
            conn: &self.conn,
            table_name: table_name.to_owned(),
        })
    }
}

impl From<Connection> for rusqlite::Connection {
    fn from(conn: Connection) -> Self {
        conn.conn
    }
}

#[must_use = "handle must be held while using temp table"]
struct TempTableDropHandle<'a> {
    conn: &'a rusqlite::Connection,
    table_name: String,
}

impl<'a> Drop for TempTableDropHandle<'a> {
    fn drop(&mut self) {
        log::trace!("dropping temp table; table_name={}", self.table_name);
        let _ignore_error = self.conn.execute(
            &format!("DROP TABLE IF EXISTS temp.{};", self.table_name),
            params![],
        );
    }
}

#[derive(Debug)]
pub struct Photoset {
    pub rowid: i64,
    pub screen_name: String,
    pub id_str: String,
    pub photo_urls: Vec<String>,
}

#[derive(Deserialize)]
struct MediaEntity {
    media_url_https: String,
    #[serde(alias = "type")]
    type_: String,
}

fn build_photoset(
    rowid: i64,
    screen_name: String,
    id_str: String,
    media_json: String,
) -> Result<Option<Photoset>> {
    match serde_json::from_str::<Option<Vec<MediaEntity>>>(&media_json) {
        Ok(Some(media)) => {
            let photo_urls: Vec<String> = media
                .into_iter()
                .filter_map(|m| {
                    if m.type_ == "photo" {
                        Some(m.media_url_https)
                    } else {
                        None
                    }
                })
                .collect();

            if photo_urls.is_empty() {
                Ok(None)
            } else {
                Ok(Some(Photoset {
                    rowid,
                    screen_name,
                    id_str,
                    photo_urls,
                }))
            }
        }
        Ok(None) => Ok(None),
        Err(e) => bail!(
            "Failed to decode media entity (rowid = {}): {:?}",
            media_json,
            e
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.create().unwrap();
        conn
    }

    #[test]
    fn must_count_tweets() {
        let conn = init_conn();

        assert_eq!(conn.count_tweets().unwrap(), 0);

        conn.inner()
            .execute_batch(
                r#"
                INSERT INTO tweets (
                    status_id,
                    content,
                    in_timeline,
                    recorded_at,
                    photos_downloaded_at
                )
                VALUES
                    ("10", "{}", 0, CURRENT_TIMESTAMP, NULL),
                    ("11", "{}", 0, CURRENT_TIMESTAMP, NULL),
                    ("12", "{}", 0, CURRENT_TIMESTAMP, NULL);
                "#,
            )
            .unwrap();

        assert_eq!(conn.count_tweets().unwrap(), 3);
    }

    #[test]
    fn must_prune_tweets() {
        let conn = init_conn();

        conn.inner()
            .execute_batch(
                r#"
                BEGIN;
                INSERT INTO tweets (
                    status_id,
                    content,
                    in_timeline,
                    recorded_at,
                    photos_downloaded_at
                )
                VALUES (
                    -- Tweet without media
                    '10',
                    json_object(
                        'user', json_object('id_str', '1', 'screen_name', 'anon')
                    ),
                    0,
                    CURRENT_TIMESTAMP,
                    NULL
                ), (
                    -- Tweet with media but no photos
                    '11',
                    json_object(
                        'user', json_object('id_str', '1', 'screen_name', 'anon'),
                        'extended_entities', json_object(
                            'media', json_array(
                                json_object('type', 'video', 'media_url_https', ''),
                                json_object('type', 'video', 'media_url_https', '')
                            )
                        )
                    ),
                    0,
                    CURRENT_TIMESTAMP,
                    NULL
                ), (
                    -- Tweet with photos and other media (photos are already downloaded)
                    '12',
                    json_object(
                        'user', json_object('id_str', '1', 'screen_name', 'anon'),
                        'extended_entities', json_object(
                            'media', json_array(
                                json_object('type', 'photo', 'media_url_https', ''),
                                json_object('type', 'video', 'media_url_https', '')
                            )
                        )
                    ),
                    0,
                    CURRENT_TIMESTAMP,
                    CURRENT_TIMESTAMP
               ), (
                    -- Tweet with photos and other media (photos are not yet downloaded)
                    '20',
                    json_object(
                        'user', json_object('id_str', '1', 'screen_name', 'anon'),
                        'extended_entities', json_object(
                            'media', json_array(
                                json_object('type', 'photo', 'media_url_https', ''),
                                json_object('type', 'video', 'media_url_https', '')
                            )
                        )
                    ),
                    0,
                    CURRENT_TIMESTAMP,
                    NULL
                );
                COMMIT;
                "#,
            )
            .unwrap();

        fn query_status_ids(conn: &Connection) -> Vec<String> {
            let mut stmt = conn
                .inner()
                .prepare("SELECT status_id FROM tweets;")
                .unwrap();
            stmt.query_map(params![], |row| row.get("status_id"))
                .unwrap()
                .flatten()
                .collect()
        }

        assert_eq!(query_status_ids(&conn), vec!["10", "11", "12", "20"]);
        assert_eq!(conn.prune_tweets().unwrap(), 3);
        assert_eq!(query_status_ids(&conn), vec!["20"]);
    }
}

#[cfg(test)]
mod schema_tests {
    use rusqlite::{params, OptionalExtension};
    use serde_json::{json, Value as JsonValue};

    use super::SCHEMA_SQL;

    #[test]
    fn schema() {
        fn has_table(conn: &rusqlite::Connection, name: &str) -> bool {
            conn.query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?;",
                params![name],
                |row| row.get(0),
            )
            .unwrap()
        }

        fn get_metadata(conn: &rusqlite::Connection, key: &str) -> Option<JsonValue> {
            conn.query_row(
                "SELECT value FROM metadata WHERE key = ?;",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .unwrap()
        }

        let conn = rusqlite::Connection::open_in_memory().unwrap();

        assert_eq!(conn.execute_batch(SCHEMA_SQL), Ok(()));
        assert!(has_table(&conn, "metadata"));
        assert!(has_table(&conn, "tweets"));
        assert!(has_table(&conn, "pruned_tweets"));
        assert_eq!(get_metadata(&conn, "schema_version"), Some(json!(0)));
    }
}
