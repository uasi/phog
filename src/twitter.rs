use std::collections::BTreeMap;
use std::ops::Deref;

use anyhow::Result;
use egg_mode::auth::{self, KeyPair, Token};
use egg_mode::user::UserID;
use linkify::{LinkFinder, LinkKind};
use regex::Regex;

use crate::config::Credentials;
use crate::egg_mode_ext::{likes, lookup, user_timeline, Timeline};

pub use crate::egg_mode_ext::Tweet;
pub use egg_mode::Response;

pub struct Client {
    token: Token,
}

impl Client {
    pub fn new(credentials: Credentials) -> Self {
        let token = Token::Access {
            consumer: KeyPair::new(credentials.consumer_key, credentials.consumer_secret),
            access: KeyPair::new(credentials.access_token, credentials.access_token_secret),
        };
        Client { token }
    }

    pub async fn fetch_likes<T: Into<UserID>>(&self, id: T) -> Result<Response<Vec<Tweet>>> {
        let response = likes(id, &self.token).await?;
        Ok(response)
    }

    pub async fn fetch_tweets(&self, status_ids: &[u64]) -> Result<Response<Vec<Tweet>>> {
        let response = lookup(status_ids.to_vec(), &self.token).await?;
        Ok(response)
    }

    pub fn user_timeline<T: Into<UserID>>(&self, id: T) -> Timeline {
        user_timeline(id, true, false, &self.token)
    }

    pub async fn verify_tokens(&self) -> Result<()> {
        Ok(auth::verify_tokens(&self.token).await.map(|_| ())?)
    }
}

pub struct UrlMap {
    map: BTreeMap<u64, String>,
}

impl UrlMap {
    pub fn extract(text: &str) -> (Self, usize) {
        let mut map = BTreeMap::new();
        let re = Regex::new(
            r"(?i)https?://(?:mobile\.|www\.)?twitter\.com/(?:[^/]+|i/web)/status(?:es)?/(\d+)",
        )
        .expect("regex must compile");
        let mut finder = LinkFinder::new();
        finder.kinds(&[LinkKind::Url]);
        let mut extracted_urls = 0;

        for link in finder.links(text) {
            extracted_urls += 1;
            let url = link.as_str();
            if let Some(cap) = re.captures(url) {
                let status_id = cap.get(1).expect("capture group must exist").as_str();
                if let Ok(status_id) = u64::from_str_radix(status_id, 10) {
                    map.insert(status_id, url.to_owned());
                }
            }
        }

        (UrlMap { map }, extracted_urls)
    }
}

impl Deref for UrlMap {
    type Target = BTreeMap<u64, String>;

    fn deref(&self) -> &BTreeMap<u64, String> {
        &self.map
    }
}

pub fn extract_screen_names(texts: &[String]) -> Vec<String> {
    let re = Regex::new(r"(?i)^(?:https?://(?:mobile\.|www\.)?twitter\.com/|@)?([0-9a-z_]+)")
        .expect("regex must compile");
    texts
        .iter()
        .filter_map(|text| {
            re.captures(text).map(|cap| {
                cap.get(1)
                    .expect("capture group must exist")
                    .as_str()
                    .to_owned()
            })
        })
        .collect()
}
