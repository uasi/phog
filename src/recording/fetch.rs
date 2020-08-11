use anyhow::{bail, Result};

use crate::common::{count, print_rate_limit};
use crate::database::Connection;
use crate::spinner::new_spinner;
use crate::twitter::{extract_screen_names, Client};

pub const MAX_DEPTH: usize = 20;

pub struct Fetch<'a> {
    db: &'a Connection,
    client: Client,
}

impl<'a> Fetch<'a> {
    pub fn new(db: &'a Connection, client: Client) -> Self {
        Self { db, client }
    }

    pub async fn from_likes(&self, screen_name_like: Vec<String>) -> Result<()> {
        let screen_names = extract_screen_names(&screen_name_like);
        for screen_name in screen_names {
            let spinner = new_spinner(&format!("Fetching likes from {}", &screen_name));
            let response = self.client.fetch_likes(screen_name.clone()).await?;
            spinner.finish_and_clear();

            print_rate_limit(&response.rate_limit_status);
            let tweets = response.response;

            println!(
                "Fetched {} from {}.",
                count(tweets.len(), "like"),
                &screen_name,
            );

            let n = self.db.insert_loose_tweets(&tweets)?;

            println!("Recorded {}.", count(n, "tweet"));
        }

        Ok(())
    }

    pub async fn from_user(
        &self,
        screen_name_like: Vec<String>,
        uses_since_id: bool,
        depth: usize,
    ) -> Result<()> {
        let screen_names = extract_screen_names(&screen_name_like);
        for screen_name in screen_names.iter() {
            log::trace!("starting fetching timeline; user={}", screen_name);

            let spinner = new_spinner(&format!("Fetching tweets from {}", &screen_name));

            let timeline = self
                .client
                .user_timeline(screen_name.clone())
                .with_page_size(200);

            let (mut timeline, response) = timeline.start().await?;
            print_rate_limit(&response.rate_limit_status);
            let mut tweets = response.response;

            log::trace!(
                "fetched timeline; user={}, page=1, tweets_in_page={}",
                screen_name,
                tweets.len()
            );

            let since_id = if uses_since_id {
                let mut since_id = None;
                if let Some(tweet) = tweets.first() {
                    if let Some(user) = &tweet.user {
                        let max_id = self.db.select_max_status_id(user.id)?;
                        since_id = max_id.map(|s| {
                            s.parse::<u64>()
                                .expect("Status ID in tweet object must be u64")
                        });
                    }
                }
                since_id
            } else {
                None
            };

            // Label on block is experimental. Use one-time loop instead.
            'fetch_more: for _once in &[1usize] {
                if let Some(since_id) = since_id {
                    if tweets.iter().all(|tweet| tweet.id <= since_id) {
                        break 'fetch_more;
                    }
                }

                let mut reached_max_depth = false;

                for page in 2..=depth {
                    log::trace!(
                        "fetching timeline; user={}, page={}, since_id={:?}",
                        screen_name,
                        page,
                        since_id
                    );

                    let (timeline2, response) = timeline.older(since_id).await?;
                    print_rate_limit(&response.rate_limit_status);
                    timeline = timeline2;
                    let older_tweets = response.response;
                    let older_tweets_len = older_tweets.len();
                    tweets.extend(older_tweets);

                    if response.rate_limit_status.remaining == 0 && older_tweets_len != 0 {
                        bail!(
                            "Rate limit exceeded while fetching tweets from {}",
                            screen_name
                        );
                    }

                    log::trace!(
                    "fetched timeline; user={}, page={}, since_id={:?}, tweets_in_page={}, total_tweets_fetched={}",
                    screen_name,
                    page,
                    since_id,
                    older_tweets_len,
                    tweets.len()
                );

                    if older_tweets_len == 0 {
                        break 'fetch_more;
                    }

                    reached_max_depth = page >= MAX_DEPTH;
                }

                if reached_max_depth {
                    // GET statuses/user_timeline should have returned up to 3200 tweets, but it returned more.
                    // https://developer.twitter.com/en/docs/tweets/timelines/api-reference/get-statuses-user_timeline
                    eprintln!(
                        "Warning: User timeline is longer than expected. Fetching stopped halfway through."
                    );
                }
            }

            spinner.finish_and_clear();

            let min_id_message = if let Some(since_id) = since_id {
                format!(", using since_id={}", since_id)
            } else {
                String::new()
            };

            println!(
                "Fetched {} from {}{}.",
                count(tweets.len(), "tweet"),
                &screen_name,
                min_id_message
            );

            let n = self.db.insert_timeline_tweets(&tweets)?;

            println!("Recorded {}.", count(n, "tweet"));
        }

        Ok(())
    }
}
