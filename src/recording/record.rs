use anyhow::Result;
use smol::block_on;

use crate::common::{count, print_rate_limit};
use crate::config;
use crate::database::Connection;
use crate::twitter::{self, UrlMap};

pub fn with_string(db: &Connection, text: String) -> Result<()> {
    let url_map = extract_url(&text)?;
    if url_map.is_empty() {
        return Ok(());
    }

    let status_ids: Vec<u64> = url_map.keys().copied().collect();
    let unseen_status_ids = {
        let mut result = db.select_unseen_status_ids_from(&status_ids)?;
        result.sort();
        result
    };

    for status_id in &status_ids {
        if !unseen_status_ids.contains(status_id) {
            let url = url_map.get(status_id).expect("status_id is in url_map");
            println!("Already recorded {}", url);
        }
    }

    let client = twitter::Client::new(config::credentials()?);
    let tweets = {
        let mut acc = Vec::with_capacity(unseen_status_ids.len());
        for chunk in unseen_status_ids.chunks(100) {
            let response = block_on(client.fetch_tweets(chunk))?;
            print_rate_limit(&response.rate_limit_status);
            acc.extend(response.response);
        }
        acc
    };

    let fetched_status_ids: Vec<u64> = tweets.iter().map(|t| t.id).collect();
    for status_id in unseen_status_ids {
        let url = url_map.get(&status_id).expect("status_id is in url_map");
        if fetched_status_ids.contains(&status_id) {
            println!("Fetched {}", url);
        } else {
            eprintln!("Warning: Could not fetch {}", url);
        }
    }

    let n = db.insert_loose_tweets(&tweets)?;
    println!("Recorded {}.", count(n, "tweet"));

    Ok(())
}

fn extract_url(text: &str) -> Result<UrlMap> {
    let (url_map, total_urls) = UrlMap::extract(text);
    println!(
        "Extracted {} out of {}.",
        count(url_map.len(), "unique status ID"),
        count(total_urls, "tweet URL"),
    );
    Ok(url_map)
}
