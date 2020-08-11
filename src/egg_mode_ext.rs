use std::convert::TryFrom;
use std::future::Future;
use std::ops::Deref;
use std::pin::Pin;
use std::task::{Context, Poll};

use egg_mode::auth;
use egg_mode::error::{self, Result};
use egg_mode::raw::{request_get as get, request_post as post, response_raw_bytes, ParamList};
use egg_mode::tweet::Tweet as TweetWithoutJson;
use egg_mode::user::UserID;
use egg_mode::{RateLimit, Response};
use hyper::{Body, Request};

type FutureResponse<T> = Pin<Box<dyn Future<Output = error::Result<Response<T>>> + Send>>;

pub struct Tweet {
    pub tweet: TweetWithoutJson,
    pub json: String,
}

impl Deref for Tweet {
    type Target = TweetWithoutJson;

    fn deref(&self) -> &Self::Target {
        &self.tweet
    }
}

pub struct Timeline {
    link: &'static str,
    token: auth::Token,
    params_base: Option<ParamList>,
    pub count: i32,
    pub max_id: Option<u64>,
    pub min_id: Option<u64>,
}

impl Timeline {
    pub fn reset(&mut self) {
        self.max_id = None;
        self.min_id = None;
    }

    pub fn start(mut self) -> TimelineFuture {
        self.reset();

        self.older(None)
    }

    pub fn older(self, since_id: Option<u64>) -> TimelineFuture {
        let req = self.request(since_id, self.min_id.map(|id| id - 1));
        let loader = Box::pin(request_with_json_response(req));

        TimelineFuture {
            timeline: Some(self),
            loader,
        }
    }

    fn request(&self, since_id: Option<u64>, max_id: Option<u64>) -> Request<Body> {
        let params = self
            .params_base
            .as_ref()
            .cloned()
            .unwrap_or_default()
            .add_param("count", self.count.to_string())
            .add_param("tweet_mode", "extended")
            .add_param("include_ext_alt_text", "true")
            .add_opt_param("since_id", since_id.map(|v| v.to_string()))
            .add_opt_param("max_id", max_id.map(|v| v.to_string()));

        get(self.link, &self.token, Some(&params))
    }

    pub fn with_page_size(self, page_size: i32) -> Self {
        Timeline {
            count: page_size,
            ..self
        }
    }

    fn map_ids(&mut self, resp: &[Tweet]) {
        self.max_id = resp.first().map(|status| status.id);
        self.min_id = resp.last().map(|status| status.id);
    }

    pub(crate) fn new(
        link: &'static str,
        params_base: Option<ParamList>,
        token: &auth::Token,
    ) -> Self {
        Timeline {
            link,
            token: token.clone(),
            params_base,
            count: 20,
            max_id: None,
            min_id: None,
        }
    }
}

#[must_use = "futures do nothing unless polled"]
pub struct TimelineFuture {
    timeline: Option<Timeline>,
    loader: FutureResponse<Vec<Tweet>>,
}

impl Future for TimelineFuture {
    type Output = Result<(Timeline, Response<Vec<Tweet>>)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        match Pin::new(&mut self.loader).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Ready(Ok(resp)) => {
                if let Some(mut timeline) = self.timeline.take() {
                    timeline.map_ids(&resp.response);
                    Poll::Ready(Ok((timeline, resp)))
                } else {
                    Poll::Ready(Err(error::Error::FutureAlreadyCompleted))
                }
            }
        }
    }
}

pub async fn likes<T: Into<UserID>>(acct: T, token: &auth::Token) -> Result<Response<Vec<Tweet>>> {
    let params = ParamList::new()
        .extended_tweets()
        .add_user_param(acct.into())
        .add_param("count", "100")
        .add_param("include_ext_alt_text", "true");

    let req = get(
        "https://api.twitter.com/1.1/favorites/list.json",
        token,
        Some(&params),
    );

    request_with_json_response(req).await
}

pub async fn lookup<I: IntoIterator<Item = u64>>(
    ids: I,
    token: &auth::Token,
) -> Result<Response<Vec<Tweet>>> {
    let id_param = ids.into_iter().fold(String::new(), |mut acc, x| {
        if !acc.is_empty() {
            acc.push(',');
        }
        acc.push_str(&x.to_string());
        acc
    });
    let params = ParamList::new()
        .extended_tweets()
        .add_param("id", id_param)
        .add_param("include_ext_alt_text", "true");

    let req = post(
        "https://api.twitter.com/1.1/statuses/lookup.json",
        token,
        Some(&params),
    );

    request_with_json_response(req).await
}

pub fn user_timeline<T: Into<UserID>>(
    acct: T,
    with_replies: bool,
    with_rts: bool,
    token: &auth::Token,
) -> Timeline {
    let params = ParamList::new()
        .extended_tweets()
        .add_user_param(acct.into())
        .add_param("exclude_replies", (!with_replies).to_string())
        .add_param("include_rts", with_rts.to_string());

    Timeline::new(
        "https://api.twitter.com/1.1/statuses/user_timeline.json",
        Some(params),
        token,
    )
}

async fn request_with_json_response(request: Request<Body>) -> Result<Response<Vec<Tweet>>> {
    let (headers, body) = response_raw_bytes(request).await?;
    let tweets: Vec<TweetWithoutJson> = serde_json::from_slice(&body)?;
    let json_values: Vec<serde_json::Value> = serde_json::from_slice(&body)?;
    let response = tweets
        .into_iter()
        .zip(json_values.into_iter())
        .map(|(tweet, json_value)| Tweet {
            tweet,
            json: serde_json::to_string(&json_value).expect("json_value must be serializable"),
        })
        .collect();
    let rate_limit_status = RateLimit::try_from(&headers)?;
    Ok(Response {
        rate_limit_status,
        response,
    })
}
