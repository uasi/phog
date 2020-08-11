BEGIN;

CREATE TABLE IF NOT EXISTS metadata (
    key TEXT NOT NULL UNIQUE,
    value TEXT NOT NULL CHECK (json_valid(value))
);

INSERT OR REPLACE INTO metadata (key, value) VALUES ('schema_version', json_quote(0));

CREATE TABLE IF NOT EXISTS tweets (
    id INTEGER PRIMARY KEY,
    status_id TEXT NOT NULL UNIQUE,
    content TEXT NOT NULL CHECK (json_valid(content)),
    in_timeline BOOLEAN NOT NULL CHECK (in_timeline IN (0, 1)),
    recorded_at DATETIME NOT NULL,
    photos_downloaded_at DATETIME
);
CREATE INDEX IF NOT EXISTS index_on_tweets_status_id ON tweets (status_id);
CREATE INDEX IF NOT EXISTS index_on_tweets_in_timeline ON tweets (in_timeline);
CREATE INDEX IF NOT EXISTS index_on_tweets_photos_downloaded_at ON tweets (photos_downloaded_at);

CREATE TABLE IF NOT EXISTS pruned_tweets (
    id INTEGER PRIMARY KEY,
    status_id TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    screen_name TEXT NOT NULL,
    media TEXT CHECK (media IS NULL OR json_valid(media)),
    in_timeline BOOLEAN NOT NULL CHECK (in_timeline IN (0, 1)),
    recorded_at DATETIME NOT NULL,
    photos_downloaded_at DATETIME,
    pruned_at DATETIME NOT NULL
);
CREATE INDEX IF NOT EXISTS index_on_pruned_tweets_status_id ON pruned_tweets (status_id);
CREATE INDEX IF NOT EXISTS index_on_pruned_tweets_user_id_status_id ON pruned_tweets (user_id, status_id);
CREATE INDEX IF NOT EXISTS index_on_pruned_tweets_in_timeline ON pruned_tweets (in_timeline);

CREATE VIEW IF NOT EXISTS seen_tweets (
    status_id,
    user_id,
    in_timeline
) AS
SELECT
    status_id,
    json_extract(tweets.content, '$.user.id_str') AS user_id,
    in_timeline
FROM tweets
UNION
SELECT
    status_id,
    user_id,
    in_timeline
FROM pruned_tweets;

COMMIT;
